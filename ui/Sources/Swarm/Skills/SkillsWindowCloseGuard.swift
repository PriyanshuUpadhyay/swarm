import AppKit
import SwiftUI

/// SwiftUI owns the window delegate. Forward its other methods while guarding an unsaved draft.
struct SkillsWindowCloseGuard: NSViewRepresentable {
    let dirty: Bool
    let attempt: (@escaping () -> Void) -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }
    func makeNSView(context: Context) -> GuardView {
        let view = GuardView()
        view.attach = { [weak coordinator = context.coordinator] window in coordinator?.attach(window) }
        return view
    }
    func updateNSView(_ view: GuardView, context: Context) {
        context.coordinator.dirty = dirty
        context.coordinator.attempt = attempt
        if let window = view.window { context.coordinator.attach(window) }
    }
    static func dismantleNSView(_ view: GuardView, coordinator: Coordinator) { coordinator.restore() }

    final class GuardView: NSView {
        var attach: ((NSWindow) -> Void)?
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window { attach?(window) }
        }
    }

    final class Coordinator: NSObject, NSWindowDelegate {
        var dirty = false
        var attempt: ((@escaping () -> Void) -> Void)?
        private weak var window: NSWindow?
        private weak var original: (any NSWindowDelegate)?
        private var allowClose = false

        func attach(_ window: NSWindow) {
            guard self.window !== window else { return }
            restore()
            self.window = window
            original = window.delegate
            window.delegate = self
        }
        func restore() {
            if let window, window.delegate === self { window.delegate = original }
            window = nil
        }
        func windowShouldClose(_ sender: NSWindow) -> Bool {
            guard dirty && !allowClose else { return original?.windowShouldClose?(sender) ?? true }
            attempt? { [weak self, weak sender] in
                self?.allowClose = true
                sender?.performClose(nil)
                self?.allowClose = false
            }
            return false
        }
        override func responds(to selector: Selector!) -> Bool {
            super.responds(to: selector) || original?.responds(to: selector) == true
        }
        override func forwardingTarget(for selector: Selector!) -> Any? {
            original?.responds(to: selector) == true ? original : super.forwardingTarget(for: selector)
        }
    }
}
