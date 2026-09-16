import AppKit
import SwiftUI

/// Whether the sidebar's table holds the keyboard in the key window, for the edge on its selection.
///
/// **Read from AppKit, because neither SwiftUI signal says it.** `backgroundProminence` inside a row
/// under a `listRowBackground` does not follow the table's focus, and `@FocusState` on the list
/// reported focused with the table demonstrably not the first responder. Both were photographed
/// before either was believed, in `69dc61d3`. The window's first responder is the fact itself.
///
/// It is a background of the list and finds the table by position rather than by walking up from
/// itself: a `.background` sits beside the list's scroll view, not inside it (see
/// `SidebarSelectionGallery.FirstResponderProbe`), and SwiftUI does not promise which of its own
/// views is whose parent. The table whose scroll view covers this view's centre is the sidebar's.
///
/// **It also switches the table's own selection drawing off, because `listRowBackground` alone did
/// not always.** The grey fill replaces AppKit's selection in the steady state, but the report came
/// back: the selected workspace solid accent blue with the keyboard edge drawn on top of it, so the
/// fill and the edge were both there and AppKit had painted the row view under them anyway. That
/// happens in a window of its own, a row view emphasised before SwiftUI has put the background
/// back, and no amount of timing on our side closes it. `selectionHighlightStyle = .none` does,
/// because then the table has no selection to draw in any state, and it is watched and put back
/// in case the list resets it when it rebuilds.
struct SidebarKeyboardFocus: NSViewRepresentable {
    var onChange: (_ listHasKeyboard: Bool, _ windowIsKey: Bool) -> Void

    func makeNSView(context: Context) -> SidebarKeyboardFocusView { SidebarKeyboardFocusView() }

    func updateNSView(_ view: SidebarKeyboardFocusView, context: Context) {
        view.onChange = onChange
    }

    static func dismantleNSView(_ view: SidebarKeyboardFocusView, coordinator: ()) {
        view.stopObserving()
    }
}

@MainActor
final class SidebarKeyboardFocusView: NSView {
    var onChange: ((Bool, Bool) -> Void)?

    private var responderObservation: NSKeyValueObservation?
    private var highlightObservation: NSKeyValueObservation?
    private var keyObservers: [NSObjectProtocol] = []
    private weak var table: NSTableView?
    private weak var observedTable: NSTableView?
    private var reported: (listHasKeyboard: Bool, windowIsKey: Bool)?

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        stopObserving()
        guard let window else {
            scheduleRefresh()
            return
        }
        // Clicking a row, Tab, the composer taking the keyboard back: every one of them moves the
        // window's first responder, so one observation covers the ways the pane gains or loses it.
        responderObservation = window.observe(\.firstResponder) { [weak self] _, _ in
            Task { @MainActor in self?.refresh() }
        }
        // Switching apps changes nothing about the first responder, only whether the window is
        // key, and that transition is the one in the report.
        let centre = NotificationCenter.default
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            keyObservers.append(centre.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
                Task { @MainActor in self?.refresh() }
            })
        }
        scheduleRefresh()
    }

    func stopObserving() {
        responderObservation?.invalidate()
        responderObservation = nil
        highlightObservation?.invalidate()
        highlightObservation = nil
        keyObservers.forEach(NotificationCenter.default.removeObserver)
        keyObservers.removeAll()
    }

    /// A turn later, never inline. `viewDidMoveToWindow` can run inside a SwiftUI update, and
    /// reporting there would be writing the list's state in the middle of the pass that placed it,
    /// which is the same reason `ListKeyboardHostView.report(focus:)` defers.
    private func scheduleRefresh() {
        Task { @MainActor [weak self] in self?.refresh() }
    }

    private func refresh() {
        guard let window else { return report(listHasKeyboard: false, windowIsKey: false) }
        let table = sidebarTable(in: window)
        if let table { suppressNativeSelection(on: table) }
        report(
            listHasKeyboard: table != nil && window.firstResponder === table,
            windowIsKey: window.isKeyWindow
        )
    }

    private func report(listHasKeyboard: Bool, windowIsKey: Bool) {
        if let reported, reported == (listHasKeyboard, windowIsKey) { return }
        reported = (listHasKeyboard, windowIsKey)
        onChange?(listHasKeyboard, windowIsKey)
    }

    /// Leaves the selection's drawing to `SidebarSelectionFill` alone. See the type's comment.
    private func suppressNativeSelection(on table: NSTableView) {
        if table.selectionHighlightStyle != .none { table.selectionHighlightStyle = .none }
        guard highlightObservation == nil || observedTable !== table else { return }
        observedTable = table
        highlightObservation = table.observe(\.selectionHighlightStyle) { [weak self] _, _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    /// Kept weakly and checked again each time, because the list can rebuild its table and a
    /// stale one would answer false for ever.
    private func sidebarTable(in window: NSWindow) -> NSTableView? {
        let centre = convert(NSPoint(x: bounds.midX, y: bounds.midY), to: nil)
        if let table, table.window === window, Self.covers(table, centre) { return table }
        table = window.contentView.flatMap { Self.table(under: $0, covering: centre) }
        return table
    }

    private static func covers(_ table: NSTableView, _ point: NSPoint) -> Bool {
        let frame: NSView = table.enclosingScrollView ?? table
        return frame.convert(frame.bounds, to: nil).contains(point)
    }

    private static func table(under view: NSView, covering point: NSPoint) -> NSTableView? {
        if let table = view as? NSTableView, covers(table, point) { return table }
        for subview in view.subviews {
            if let found = table(under: subview, covering: point) { return found }
        }
        return nil
    }
}
