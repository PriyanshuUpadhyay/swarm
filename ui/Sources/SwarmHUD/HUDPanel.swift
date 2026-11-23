import AppKit
import SwiftUI

final class HUDPanel: NSPanel {
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
