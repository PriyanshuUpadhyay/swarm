import AppKit
import Carbon.HIToolbox
import SwarmHUDCore
import SwiftUI

// Window code ported from yelo apps/UsageHUD (AppDelegate.swift, DragHostingView.swift): a fixed,
// transparent panel at the top of the notch screen; the visible shape grows inside it. Hover comes
// from mouse-move monitors; a global monitor of mouse events needs no input-monitoring permission.
// ⌃⌥⌘U (a Carbon hot key, also with no permission) opens it for the keyboard and VoiceOver.

/// The panel never resizes; it is the most room the open HUD may take.
let panelSize = NSSize(width: 800, height: 640)

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let model = UsageModel()
    private var panel: HUDPanel!
    private var hostingView: HitTestHostingView<HUDView>!

    // The exact set is re-asserted after unlock and during wedge recovery (below).
    private let collectionBehavior: NSWindow.CollectionBehavior = [
        .canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle,
    ]
    private let openDelay: TimeInterval = 0.12
    private let closeDelay: TimeInterval = 0.35
    private let visibilityCheckDelay: TimeInterval = 1.0
    private let maxWedgeRecoveries = 3

    private var pendingOpen: DispatchWorkItem?
    private var pendingClose: DispatchWorkItem?
    private var pendingVisibilityCheck: DispatchWorkItem?
    private var wedgeRecoveries = 0
    private var mouseMonitors: [Any] = []
    private var hotKey: EventHotKeyRef?
    /// Opened by the hot key: it stays open when the cursor leaves, until Esc, the key again, or a click elsewhere.
    private var keyOpen = false
    /// Closed by the keyboard with the cursor on the notch: hover may not reopen it until the cursor leaves.
    private var hoverHeld = false
    private var readTimer: Timer?
    private var phaseTimer: Timer?
    private var clickThrough: Bool?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        buildPanel()
        reposition()
        model.read()
        readTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { _ in
            MainActor.assumeIsolated { self.model.read() }
        }
        readTimer?.tolerance = 10
        schedulePhaseChange()
        NotificationCenter.default.addObserver(
            self, selector: #selector(screensChanged), name: NSApplication.didChangeScreenParametersNotification, object: nil
        )
        // After an overnight lock the window can lose its all-spaces tag; re-assert it on unlock.
        DistributedNotificationCenter.default().addObserver(
            self, selector: #selector(unlocked), name: NSNotification.Name("com.apple.screenIsUnlocked"), object: nil
        )
        // A new time zone or a clock change moves the next theme boundary. Foundation may post these
        // off the main thread, so they are delivered on the main queue.
        for name in [Notification.Name.NSSystemTimeZoneDidChange, .NSSystemClockDidChange] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { _ in
                MainActor.assumeIsolated { self.schedulePhaseChange() }
            }
        }
        panel.orderFrontRegardless()
        startMouseMonitors()
        registerHotKey()
        scheduleVisibilityCheck()
    }

    // MARK: Theme by the clock

    private func schedulePhaseChange() {
        model.updatePhase()
        phaseTimer?.invalidate()
        let timer = Timer(fire: DayPhase.nextChange(after: Date()), interval: 0, repeats: false) { _ in
            MainActor.assumeIsolated { self.schedulePhaseChange() }
        }
        RunLoop.main.add(timer, forMode: .common)
        phaseTimer = timer
    }

    // MARK: Panel

    private func buildPanel() {
        panel = HUDPanel(
            contentRect: NSRect(origin: .zero, size: panelSize),
            styleMask: [.nonactivatingPanel, .borderless, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = true
        panel.level = .mainMenu + 3
        panel.collectionBehavior = collectionBehavior
        panel.hidesOnDeactivate = false
        panel.isMovable = false
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.acceptsMouseMovedEvents = true
        panel.onCancel = { [weak self] in self?.closeFromKeyboard() }
        panel.onResignKey = { [weak self] in self?.closeFromKeyboard() }
        clickThrough = nil

        let view = HitTestHostingView(rootView: HUDView(model: model, live: true))
        view.activeRect = { [weak self] in self?.visibleRectInScreen() ?? .zero }
        view.makeMenu = { [weak self] in self?.contextMenu() }
        model.toggleFromKeyboard = { [weak self] in self?.toggleFromKeyboard() }
        hostingView = view
        panel.contentView = view
    }

    /// The built-in notched display is home; with no notch, the screen under the cursor.
    private func notchScreen() -> NSScreen? {
        NSScreen.screens.first {
            guard let id = $0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID else { return false }
            return CGDisplayIsBuiltin(id) != 0 && $0.safeAreaInsets.top > 0
        } ?? NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) } ?? NSScreen.screens.first
    }

    private func reposition() {
        // No screen at all (lid closed, no display): keep the old frame until one returns.
        guard let screen = notchScreen() else { return }
        let menuBar = screen.frame.maxY - screen.visibleFrame.maxY
        if screen.safeAreaInsets.top > 0 {
            let left = screen.auxiliaryTopLeftArea?.width ?? 0
            let right = screen.auxiliaryTopRightArea?.width ?? 0
            model.notch = CGSize(width: screen.frame.width - left - right, height: screen.safeAreaInsets.top)
        } else {
            // No notch: a pill as tall as the menu bar, so it reads as part of it.
            model.notch = CGSize(width: 0, height: max(menuBar, 24))
        }
        panel.setFrame(NSRect(x: screen.frame.midX - panelSize.width / 2, y: screen.frame.maxY - panelSize.height,
                              width: panelSize.width, height: panelSize.height), display: false)
    }

    /// The shape the owner sees, in screen coordinates: the closed wings or the open card.
    private func visibleRectInScreen() -> NSRect {
        let size = model.isOpen ? model.openSize : model.closedSize
        let frame = panel.frame
        return NSRect(x: frame.midX - size.width / 2, y: frame.maxY - size.height, width: size.width, height: size.height)
    }

    /// A few points of slack around the closed shape and the top edge, so a cursor pushed against
    /// the top of the screen still counts as inside.
    private func hoverRectInScreen() -> NSRect {
        let rect = visibleRectInScreen()
        return model.isOpen ? NSRect(x: rect.minX, y: rect.minY, width: rect.width, height: rect.height + 1)
                            : rect.insetBy(dx: -8, dy: -4)
    }

    @objc private func screensChanged() {
        reposition()
        mouseMoved()
    }

    @objc private func unlocked() {
        // A new unlock earns new tries, so a slow wake that used them all does not hide the HUD for good.
        wedgeRecoveries = 0
        reassertCollectionBehavior()
        scheduleVisibilityCheck()
    }

    // MARK: Hover

    /// The global monitor sees moves over other apps (the panel is click-through there); the local
    /// one sees moves over the panel itself. Both run on the main thread.
    private func startMouseMonitors() {
        let events: NSEvent.EventTypeMask = [.mouseMoved, .leftMouseDragged]
        if let global = NSEvent.addGlobalMonitorForEvents(matching: events, handler: { _ in
            MainActor.assumeIsolated { self.mouseMoved() }
        }) { mouseMonitors.append(global) }
        if let local = NSEvent.addLocalMonitorForEvents(matching: events, handler: { event in
            MainActor.assumeIsolated { self.mouseMoved() }
            return event
        }) { mouseMonitors.append(local) }
    }

    /// The one place hover and click-through are decided, from the cursor position.
    private func mouseMoved() {
        guard panel.isVisible else { return }
        let inside = hoverRectInScreen().contains(NSEvent.mouseLocation)
        setClickThrough(!inside)
        if keyOpen { return }
        if hoverHeld {
            if inside { return }
            hoverHeld = false
        }
        if !model.isOpen {
            if inside { scheduleOpen() } else { pendingOpen?.cancel(); pendingOpen = nil }
        } else if inside {
            pendingClose?.cancel(); pendingClose = nil
        } else if pendingClose == nil {
            scheduleClose()
        }
    }

    private func scheduleOpen() {
        guard pendingOpen == nil else { return }
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingOpen = nil
            guard !self.model.isOpen, self.hoverRectInScreen().contains(NSEvent.mouseLocation) else { return }
            self.setOpen(true)
        }
        pendingOpen = item
        DispatchQueue.main.asyncAfter(deadline: .now() + openDelay, execute: item)
    }

    private func scheduleClose() {
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingClose = nil
            guard !self.keyOpen, !self.hoverRectInScreen().contains(NSEvent.mouseLocation) else { return }
            self.setOpen(false)
        }
        pendingClose = item
        DispatchQueue.main.asyncAfter(deadline: .now() + closeDelay, execute: item)
    }

    private func setOpen(_ open: Bool) {
        guard model.isOpen != open else { return }
        pendingOpen?.cancel(); pendingOpen = nil
        model.isOpen = open
        if open {
            NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
            // The minute timer keeps it current; this only catches up after a long closed spell.
            if model.readAt.map({ Date().timeIntervalSince($0) > 5 }) ?? true { model.read() }
        } else {
            keyOpen = false
            // A nonactivating panel keeps key status until it leaves the screen, and the owner's
            // next keys would come here, not to their app; ordering it out and in hands it back.
            if panel.isKeyWindow {
                panel.orderOut(nil)
                panel.orderFrontRegardless()
            }
        }
        mouseMoved()
    }

    // MARK: Keyboard

    private func registerHotKey() {
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let handler = InstallEventHandler(GetApplicationEventTarget(), { _, _, context in
            guard let context else { return noErr }
            let delegate = Unmanaged<AppDelegate>.fromOpaque(context).takeUnretainedValue()
            MainActor.assumeIsolated { delegate.toggleFromKeyboard() }
            return noErr
        }, 1, &spec, Unmanaged.passUnretained(self).toOpaque(), nil)
        let key = RegisterEventHotKey(UInt32(kVK_ANSI_U), UInt32(controlKey | optionKey | cmdKey),
                                      EventHotKeyID(signature: OSType(0x5357_4855), id: 1), // "SWHU"
                                      GetApplicationEventTarget(), 0, &hotKey)
        // Another app may hold the key already; then the VoiceOver hint stops naming it.
        model.hotKeyReady = handler == noErr && key == noErr
        if !model.hotKeyReady { NSLog("SwarmHUD: ⌃⌥⌘U is not available (handler %d, key %d)", handler, key) }
    }

    func toggleFromKeyboard() {
        if keyOpen { closeFromKeyboard(); return }
        pendingClose?.cancel(); pendingClose = nil
        keyOpen = true
        setOpen(true)
        // Key focus lets Esc close it and moves VoiceOver into the card.
        panel.makeKey()
    }

    private func closeFromKeyboard() {
        guard keyOpen else { return }
        keyOpen = false
        hoverHeld = hoverRectInScreen().contains(NSEvent.mouseLocation)
        setOpen(false)
    }

    private func setClickThrough(_ ignore: Bool) {
        guard clickThrough != ignore else { return }
        clickThrough = ignore
        panel.ignoresMouseEvents = ignore
    }

    // MARK: Menu (right click)

    private func contextMenu() -> NSMenu {
        let menu = NSMenu()
        // Else the menu enables every item whose target answers, and the refresh state is lost.
        menu.autoenablesItems = false
        let refresh = NSMenuItem(title: "Refresh usage", action: #selector(refreshUsage), keyEquivalent: "r")
        refresh.target = self
        refresh.isEnabled = model.canRefresh
        menu.addItem(refresh)
        menu.addItem(.separator())
        let theme = NSMenuItem(title: "Theme", action: nil, keyEquivalent: "")
        let themes = NSMenu()
        for (title, phase) in [("Follow the clock", nil)] + DayPhase.allCases.map({ ($0.title, Optional($0)) }) {
            let item = NSMenuItem(title: title, action: #selector(pickTheme(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = phase?.rawValue
            item.state = model.pinnedPhase == phase ? .on : .off
            themes.addItem(item)
        }
        theme.submenu = themes
        menu.addItem(theme)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Usage HUD", action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
        return menu
    }

    @objc private func refreshUsage() { model.refresh() }

    @objc private func pickTheme(_ item: NSMenuItem) {
        model.pinnedPhase = (item.representedObject as? String).flatMap(DayPhase.init(rawValue:))
    }

    /// A clean exit, so the LaunchAgent (KeepAlive.SuccessfulExit = false) does not restart it.
    @objc private func quit() { NSApp.terminate(nil) }

    // MARK: Lock-screen wedge recovery
    //
    // After an overnight lock the panel's WindowServer window can lose its all-spaces tag: the app
    // thinks it is visible, but it never reaches the active Space. Re-tag it; if that fails, rebuild.

    private func panelIsOnScreen() -> Bool {
        guard panel.windowNumber > 0,
              let list = CGWindowListCopyWindowInfo([.optionIncludingWindow], CGWindowID(panel.windowNumber)) as? [[String: AnyObject]],
              let info = list.first else { return false }
        return (info[kCGWindowIsOnscreen as String] as? Bool) ?? false
    }

    /// Through [] first, because assigning the value the window already holds may not re-tag it.
    private func reassertCollectionBehavior() {
        panel.collectionBehavior = []
        panel.collectionBehavior = collectionBehavior
    }

    private func scheduleVisibilityCheck() {
        pendingVisibilityCheck?.cancel()
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingVisibilityCheck = nil
            guard self.panel.isVisible else { return }
            if self.panelIsOnScreen() { self.wedgeRecoveries = 0 } else { self.recover(rebuild: false) }
        }
        pendingVisibilityCheck = item
        DispatchQueue.main.asyncAfter(deadline: .now() + visibilityCheckDelay, execute: item)
    }

    private func recover(rebuild: Bool) {
        guard wedgeRecoveries < maxWedgeRecoveries else { return }
        wedgeRecoveries += 1
        if rebuild {
            let frame = panel.frame
            panel.orderOut(nil)
            buildPanel()
            panel.setFrame(frame, display: false)
        } else {
            panel.orderOut(nil)
            reassertCollectionBehavior()
        }
        panel.orderFrontRegardless()
        DispatchQueue.main.asyncAfter(deadline: .now() + visibilityCheckDelay) { [weak self] in
            guard let self, self.panel.isVisible else { return }
            if self.panelIsOnScreen() { self.wedgeRecoveries = 0 } else { self.recover(rebuild: !rebuild) }
        }
    }
}

private final class HUDPanel: NSPanel {
    var onCancel: (() -> Void)?
    var onResignKey: (() -> Void)?
    override var canBecomeKey: Bool { true }
    override func cancelOperation(_ sender: Any?) { onCancel?() }
    override func resignKey() {
        super.resignKey()
        onResignKey?()
    }
}

/// Hosts the SwiftUI content and limits hit-testing to the visible shape, so the rest of the
/// transparent panel never takes a click.
final class HitTestHostingView<Content: View>: NSHostingView<Content> {
    var activeRect: (() -> NSRect)?
    var makeMenu: (() -> NSMenu?)?

    // A nonactivating panel of an accessory app gets every click as a first mouse.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let window, let rect = activeRect?() else { return nil }
        return window.convertFromScreen(rect).contains(point) ? super.hitTest(point) : nil
    }

    override func menu(for event: NSEvent) -> NSMenu? { makeMenu?() }
}
