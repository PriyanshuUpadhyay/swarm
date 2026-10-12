import AppKit
import OSLog
import SwiftUI
import SwarmCore

/// Owns the usage HUD: a panel at the top of the screen under the pointer, the hover rule that
/// opens and closes it, and the slow read of cached usage. The panel exists only while the owner
/// has the HUD on, and only while the app runs.
@MainActor @Observable
final class UsageHUDWindow {
    /// One per process: the launch hook and every window must reach the same panel.
    static let shared = UsageHUDWindow()

    /// The Accounts page uses the same refresher, so the two share the 300-second limit.
    let refresher: UsageRefresher
    let model: UsageHUDModel
    /// Set by the app's menu commands, because the Settings action comes from the environment
    /// and the menu bar exists when no window does (ADR 0069).
    @ObservationIgnored var openAccounts: () -> Void = {}
    /// The "Add the usage HUD?" alert shows at most one time per app run.
    @ObservationIgnored var askedThisRun = false

    private var panel: NSPanel?
    @ObservationIgnored private var hostingView: UsageHUDHostingView<UsageHUDPanel>?
    @ObservationIgnored private var placedScreenFrame: CGRect?
    @ObservationIgnored private var background: [Task<Void, Never>] = []
    @ObservationIgnored private var pendingExpand: Task<Void, Never>?
    @ObservationIgnored private var pendingCollapse: Task<Void, Never>?
    @ObservationIgnored private var pendingVisibilityCheck: Task<Void, Never>?
    @ObservationIgnored private var clickThrough: Bool?
    private let logger = Logger(subsystem: "io.github.priyanshuupadhyay.swarm", category: "usage-hud")

    private static let expandDelay = Duration.seconds(0.3)
    private static let collapseDelay = Duration.seconds(0.5)
    private static let pollInterval = Duration.seconds(120)
    /// Time for WindowServer to put a shown window on the active Space.
    private static let visibilityCheckDelay = Duration.seconds(1)
    private static let maxWedgeRecoveries = 3
    /// The window must stay on every Space; see `scheduleVisibilityCheck`.
    private static let collectionBehavior: NSWindow.CollectionBehavior = [
        .canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle,
    ]

    private init() {
        let refresher = UsageRefresher { provider in
            await LoginShellPath.ready()
            return try await SwarmCLIProfileSource().refreshUsage(provider: provider)
        }
        self.refresher = refresher
        model = UsageHUDModel(
            loadUsage: {
                await LoginShellPath.ready()
                return try await SwarmCLIProfileSource().usage()
            },
            refresher: refresher
        )
    }

    // MARK: On and off

    /// Shows the HUD when the app has started, with or without a window. Call it before the app
    /// runs; a panel cannot be made earlier than that.
    func showAtLaunch() {
        // Registered now and not in a Task, because a Task can start after the notification.
        // The notification comes one time per process, so the observer stays.
        _ = NotificationCenter.default.addObserver(
            forName: NSApplication.didFinishLaunchingNotification, object: nil, queue: .main
        ) { _ in
            MainActor.assumeIsolated {
                Self.shared.setShown(UsageHUDPrompt.showsAtLaunch(choices: OwnerChoicesStore()))
            }
        }
    }

    func setShown(_ shown: Bool) {
        guard shown != (panel != nil) else { return }
        guard shown else {
            removePanel()
            return
        }
        let panel = buildPanel()
        placeUnderPointer()
        panel.orderFrontRegardless()
        background = [
            Task { [weak self] in
                while !Task.isCancelled {
                    await self?.model.read()
                    try? await Task.sleep(for: Self.pollInterval)
                }
            },
            // Sampling the pointer needs no event monitor of other apps and no input permission.
            Task { [weak self] in
                while !Task.isCancelled {
                    self?.pointerMoved()
                    try? await Task.sleep(for: .milliseconds(100))
                }
            },
            Task { [weak self] in
                let changes = NotificationCenter.default
                    .notifications(named: NSApplication.didChangeScreenParametersNotification).map { _ in () }
                for await _ in changes {
                    self?.placeUnderPointer()
                    self?.hostingView?.updateTrackingAreas()
                    self?.pointerMoved()
                }
            },
            // After a long lock the window can lose its all-Spaces tag; set it again on unlock.
            Task { [weak self] in
                let unlocks = DistributedNotificationCenter.default()
                    .notifications(named: Notification.Name("com.apple.screenIsUnlocked")).map { _ in () }
                for await _ in unlocks { self?.reassertCollectionBehavior() }
            },
        ]
        scheduleVisibilityCheck()
    }

    /// The View menu item. A panel opened here stays open until Esc, Close, or this item again.
    func toggleFromMenu() {
        guard let panel else { return }
        guard model.isCollapsed else {
            close()
            return
        }
        model.openedFromMenu = true
        placeUnderPointer()
        expand()
        // Key, so that Esc and ⌘R reach the panel.
        panel.makeKeyAndOrderFront(nil)
        // The panel takes focus with no window change that VoiceOver says, so say that it opened.
        AccessibilityNotification.Announcement("Usage HUD open. \(model.headerCaption)").post()
    }

    private func close() {
        model.openedFromMenu = false
        collapse()
    }

    private func removePanel() {
        for task in background + [pendingExpand, pendingCollapse, pendingVisibilityCheck].compactMap({ $0 }) {
            task.cancel()
        }
        background = []
        pendingExpand = nil
        pendingCollapse = nil
        pendingVisibilityCheck = nil
        panel?.orderOut(nil)
        panel = nil
        hostingView = nil
        placedScreenFrame = nil
        model.openedFromMenu = false
        model.isResizing = false
        model.isCollapsed = true
    }

    // MARK: Panel

    @discardableResult
    private func buildPanel() -> NSPanel {
        let panel = KeyablePanel(
            contentRect: CGRect(origin: .zero, size: model.panelSize),
            styleMask: [.nonactivatingPanel, .borderless, .fullSizeContentView],
            backing: .buffered, defer: false
        )
        panel.isFloatingPanel = true
        // Above the menu bar, so the collapsed panel can sit on the notch.
        panel.level = .mainMenu + 3
        panel.collectionBehavior = Self.collectionBehavior
        panel.hidesOnDeactivate = false
        panel.isMovable = false
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.acceptsMouseMovedEvents = true
        clickThrough = nil

        let actions = UsageHUDActions(
            refresh: { [weak self] in
                guard let self else { return }
                Task { await self.model.refresh() }
            },
            close: { [weak self] in self?.close() },
            openAccounts: { [weak self] in
                self?.close()
                // A hover can open the panel while another app is active.
                NSApp.activate()
                self?.openAccounts()
            },
            resizeEnded: { [weak self] in
                self?.hostingView?.updateTrackingAreas()
                self?.pointerMoved()
            }
        )
        let view = UsageHUDHostingView(rootView: UsageHUDPanel(model: model, actions: actions))
        view.activeRectInScreen = { [weak self] in
            guard let self else { return .zero }
            return self.contentRectInScreen(isCollapsed: self.model.isCollapsed)
        }
        view.trackingRectInScreen = { [weak self] in
            guard let self else { return .zero }
            return self.model.isCollapsed ? self.collapsedHoverRect() : self.openHoverRect()
        }
        view.onPointerMoved = { [weak self] in self?.pointerMoved() }
        hostingView = view
        panel.contentView = view
        self.panel = panel
        return panel
    }

    /// The screen under the pointer, not `NSScreen.main`: the main screen follows the key window,
    /// which can be on another screen than the one the owner looks at.
    private func placeUnderPointer() {
        let pointer = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { $0.frame.contains(pointer) })
                ?? NSScreen.screens.first(where: { $0.frame == placedScreenFrame })
                ?? NSScreen.screens.first else { return }
        place(on: screen)
    }

    private func place(on screen: NSScreen) {
        guard let panel else { return }
        model.applyPanelCeiling(UsageHUDLayout.panelHeight(screenHeight: screen.visibleFrame.height))
        model.applyScreenWidth(screen.frame.width)
        let menuBarHeight = screen.frame.maxY - screen.visibleFrame.maxY
        model.hasNotch = screen.safeAreaInsets.top > 0
        if model.hasNotch {
            model.notchTopInset = max(screen.safeAreaInsets.top, menuBarHeight)
            model.notchTriggerWidth = screen.frame.width
                - (screen.auxiliaryTopLeftArea?.width ?? 0) - (screen.auxiliaryTopRightArea?.width ?? 0)
        } else {
            // No notch: a pill on the menu bar at the top center. A hidden menu bar has no height,
            // so the pill then takes the height of a status item.
            model.notchTopInset = max(menuBarHeight, NSStatusBar.system.thickness)
            model.notchTriggerWidth = UsageHUDLayout.fallbackNotchSize.width
        }
        let origin = UsageHUDLayout.notchAnchoredOrigin(size: model.panelSize, screenFrame: screen.frame)
        panel.setFrame(CGRect(origin: origin, size: model.panelSize), display: false)
        placedScreenFrame = screen.frame
    }

    // MARK: Hover rule

    private func contentRectInScreen(isCollapsed: Bool) -> CGRect {
        guard let panel else { return .zero }
        return UsageHUDLayout.contentRect(
            isCollapsed: isCollapsed, expandedHeight: model.expandedContentHeight, expandedWidth: model.panelWidth,
            notchWidth: model.notchTriggerWidth, notchHeight: model.notchTopInset, panelSize: model.panelSize
        )
        .offsetBy(dx: panel.frame.origin.x, dy: panel.frame.origin.y)
    }

    private func collapsedHoverRect() -> CGRect {
        guard let panel else { return .zero }
        return UsageHUDLayout.fullNotchHoverRect(UsageHUDLayout.physicalNotchRect(
            panelFrame: panel.frame, notchWidth: model.notchTriggerWidth, notchHeight: model.notchTopInset
        ))
    }

    private func openHoverRect() -> CGRect {
        UsageHUDLayout.topEdgeInclusiveRect(contentRectInScreen(isCollapsed: false))
    }

    /// The one place that decides hover state and click-through, from the pointer's screen position.
    private func pointerMoved() {
        // A drag can leave the card at a width limit. The panel must not collapse then, or turn
        // click-through, because that drops the rest of the drag.
        guard let panel, panel.isVisible, !model.isResizing else { return }
        let pointer = NSEvent.mouseLocation
        // The collapsed panel follows the pointer to another screen.
        if model.isCollapsed, let screen = NSScreen.screens.first(where: { $0.frame.contains(pointer) }),
           screen.frame != placedScreenFrame {
            place(on: screen)
            hostingView?.updateTrackingAreas()
        }
        let inside = (model.isCollapsed ? collapsedHoverRect() : openHoverRect()).contains(pointer)
        if clickThrough != !inside {
            clickThrough = !inside
            panel.ignoresMouseEvents = !inside
        }
        if model.isCollapsed {
            if inside {
                scheduleExpand()
            } else {
                pendingExpand?.cancel()
                pendingExpand = nil
            }
        } else if inside {
            pendingCollapse?.cancel()
            pendingCollapse = nil
        } else if pendingCollapse == nil {
            scheduleCollapse()
        }
    }

    private func scheduleExpand() {
        guard pendingExpand == nil else { return }
        pendingExpand = Task { [weak self] in
            try? await Task.sleep(for: Self.expandDelay)
            guard !Task.isCancelled, let self else { return }
            self.pendingExpand = nil
            guard self.model.isCollapsed, self.collapsedHoverRect().contains(NSEvent.mouseLocation) else { return }
            self.expand()
        }
    }

    private func scheduleCollapse() {
        guard !model.openedFromMenu else { return }
        pendingCollapse = Task { [weak self] in
            try? await Task.sleep(for: Self.collapseDelay)
            guard !Task.isCancelled, let self else { return }
            self.pendingCollapse = nil
            guard !self.model.openedFromMenu, !self.model.isResizing,
                  !self.openHoverRect().contains(NSEvent.mouseLocation) else { return }
            self.collapse()
        }
    }

    private func expand() {
        guard model.isCollapsed else { return }
        pendingExpand?.cancel()
        pendingExpand = nil
        model.isCollapsed = false
        hostingView?.updateTrackingAreas()
        pointerMoved()
        // The cached usage is read each time the panel opens.
        Task { await model.read() }
    }

    private func collapse() {
        guard !model.isCollapsed else { return }
        pendingExpand?.cancel()
        pendingExpand = nil
        model.isCollapsed = true
        // A panel opened from the menu took the keys. Give them back to a window of the app.
        if let panel, panel.isKeyWindow {
            (NSApp.mainWindow ?? NSApp.windows.first { $0 !== panel && $0.isVisible && $0.canBecomeKey })?.makeKey()
        }
        hostingView?.updateTrackingAreas()
        pointerMoved()
    }

    // MARK: A window that is shown but not on screen
    //
    // After a long screen lock, the WindowServer window can lose its all-Spaces tag and stay on an
    // old Space. `isVisible` is then true, but the window is not on the active Space. Another
    // process cannot move it, so the app sets the tag again and, if that fails, makes a new panel.

    private func panelIsOnScreen() -> Bool {
        guard let panel, panel.windowNumber > 0,
              let list = CGWindowListCopyWindowInfo([.optionIncludingWindow], CGWindowID(panel.windowNumber))
                as? [[String: Any]],
              let info = list.first else { return false }
        return info[kCGWindowIsOnscreen as String] as? Bool ?? false
    }

    /// Through `[]`, because a write of the same value can be a no-op that sets no tag.
    private func reassertCollectionBehavior() {
        panel?.collectionBehavior = []
        panel?.collectionBehavior = Self.collectionBehavior
    }

    private func scheduleVisibilityCheck() {
        pendingVisibilityCheck?.cancel()
        pendingVisibilityCheck = Task { [weak self] in
            for attempt in 0...Self.maxWedgeRecoveries {
                try? await Task.sleep(for: Self.visibilityCheckDelay)
                guard !Task.isCancelled, let self, let panel = self.panel, panel.isVisible,
                      !self.panelIsOnScreen() else { return }
                guard attempt < Self.maxWedgeRecoveries else {
                    self.logger.error("The usage HUD is not on the active Space after \(attempt) recoveries")
                    return
                }
                if attempt.isMultiple(of: 2) {
                    self.logger.notice("The usage HUD is not on the active Space; setting its Spaces tag again")
                    panel.orderOut(nil)
                    self.reassertCollectionBehavior()
                    panel.orderFrontRegardless()
                } else {
                    self.logger.notice("The usage HUD is not on the active Space; making a new panel")
                    let frame = panel.frame
                    panel.orderOut(nil)
                    // The old view goes away with any drag in progress.
                    self.model.isResizing = false
                    let replacement = self.buildPanel()
                    replacement.setFrame(frame, display: false)
                    replacement.orderFrontRegardless()
                    self.hostingView?.updateTrackingAreas()
                    self.pointerMoved()
                }
            }
        }
    }
}

/// A borderless panel refuses the keys by default, and then Esc and ⌘R do not reach it.
private final class KeyablePanel: NSPanel {
    override var canBecomeKey: Bool { true }
}

/// Hosts the SwiftUI content, limits hit-testing to the visible part of the window, and owns the
/// tracking area that covers the notch while another app is active.
private final class UsageHUDHostingView<Content: View>: NSHostingView<Content> {
    var activeRectInScreen: (() -> CGRect)?
    var trackingRectInScreen: (() -> CGRect)?
    var onPointerMoved: (() -> Void)?

    private var activeTrackingArea: NSTrackingArea?

    // A click in a panel that does not activate the app is always a first click. The default view
    // refuses it, and then a button never gets its press.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let window, let activeRect = activeRectInScreen?() else { return nil }
        // `point` is in window coordinates already. A conversion through this flipped view would
        // mirror the controls at the top to the bottom.
        return window.convertFromScreen(activeRect).contains(point) ? super.hitTest(point) : nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updateTrackingAreas()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let activeTrackingArea {
            removeTrackingArea(activeTrackingArea)
            self.activeTrackingArea = nil
        }
        guard let window, let screenRect = trackingRectInScreen?(), !screenRect.isEmpty else { return }
        let area = NSTrackingArea(
            rect: convert(window.convertFromScreen(screenRect), from: nil),
            options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .enabledDuringMouseDrag],
            owner: self
        )
        addTrackingArea(area)
        activeTrackingArea = area
    }

    override func mouseEntered(with event: NSEvent) { onPointerMoved?() }
    override func mouseMoved(with event: NSEvent) { onPointerMoved?() }
    override func mouseExited(with event: NSEvent) { onPointerMoved?() }
}
