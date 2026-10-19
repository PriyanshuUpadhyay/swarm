// SwarmApp: Swarm.app's executable, one full-screen window with one WKWebView on the `swarm ui`
// server. `swarm ui` starts it with `open -a Swarm.app --args <url>`; when it already runs, that
// `open` only activates it and its window comes to the front.
// usage: SwarmApp <http://127.0.0.1:PORT/#token=HEX>
import AppKit
import WebKit

@MainActor
final class Shell: NSObject, NSApplicationDelegate, NSWindowDelegate, WKUIDelegate {
    let url: URL?
    let webView = WKWebView(frame: .zero)
    var window: NSWindow!
    /// Paints the camera row above the full-screen window in the page's background colour. A
    /// full-screen window never covers that row; macOS leaves it black.
    var strip: NSPanel?
    var stripColor: NSKeyValueObservation?
    var startedFullScreen = false

    init(url: URL?) {
        self.url = url
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.mainMenu = makeMenu()
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Swarm"
        window.center()
        window.collectionBehavior = .fullScreenPrimary
        window.delegate = self
        window.backgroundColor = NSColor(red: 0x10 / 255, green: 0x10 / 255, blue: 0x10 / 255, alpha: 1)
        webView.uiDelegate = self
        window.contentView = webView
        window.makeFirstResponder(webView)
        window.makeKeyAndOrderFront(nil)
        if let url, url.host == "127.0.0.1" {
            webView.load(URLRequest(url: url))
        } else {
            webView.loadHTMLString("<p style=\"font: 13px -apple-system; padding: 16px\">Run <code>swarm ui</code> to open Swarm.</p>", baseURL: nil)
        }
        NSApp.activate()
    }

    /// Full screen needs the active app. macOS may refuse the activation of a launch from a process
    /// in the background, so the window goes full screen at the first activation, not at launch.
    func applicationDidBecomeActive(_ notification: Notification) {
        guard !startedFullScreen else { return }
        startedFullScreen = true
        window.toggleFullScreen(nil)
    }

    /// `open` on the running app: show the window, also when it is minimized.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        window.deminiaturize(nil)
        window.makeKeyAndOrderFront(nil)
        return false
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func windowDidEnterFullScreen(_ notification: Notification) {
        guard let screen = window.screen, screen.safeAreaInsets.top > 0 else { return }
        let height = screen.safeAreaInsets.top
        let panel = StripPanel(
            contentRect: NSRect(x: screen.frame.minX, y: screen.frame.maxY - height, width: screen.frame.width, height: height),
            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.ignoresMouseEvents = true
        panel.hasShadow = false
        panel.collectionBehavior = .fullScreenAuxiliary
        stripColor = webView.observe(\.underPageBackgroundColor, options: [.initial, .new]) { webView, _ in
            MainActor.assumeIsolated { panel.backgroundColor = webView.underPageBackgroundColor }
        }
        // A child window stays in its parent's full-screen space and closes with it.
        window.addChildWindow(panel, ordered: .above)
        strip = panel
    }

    func windowWillExitFullScreen(_ notification: Notification) {
        stripColor = nil
        if let strip { window.removeChildWindow(strip); strip.close() }
        strip = nil
    }

    /// A `target="_blank"` link opens in the default browser.
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if let url = navigationAction.request.url, ["http", "https"].contains(url.scheme) {
            NSWorkspace.shared.open(url)
        }
        return nil
    }

    func makeMenu() -> NSMenu {
        let main = NSMenu()
        let app = NSMenu()
        app.addItem(withTitle: "Hide Swarm", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        app.addItem(withTitle: "Quit Swarm", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        // The web view's text fields get ⌘C, ⌘V and the rest only through these menu items. Other
        // keys reach the page, which owns the key map.
        let edit = NSMenu(title: "Edit")
        for (title, action, key, mods) in [
            ("Undo", "undo:", "z", NSEvent.ModifierFlags.command), ("Redo", "redo:", "z", [.command, .shift]),
            ("Cut", "cut:", "x", .command), ("Copy", "copy:", "c", .command), ("Paste", "paste:", "v", .command),
            ("Select All", "selectAll:", "a", .command),
        ] {
            edit.addItem(withTitle: title, action: Selector(action), keyEquivalent: key).keyEquivalentModifierMask = mods
        }
        let view = NSMenu(title: "View")
        view.addItem(withTitle: "Toggle Full Screen", action: #selector(NSWindow.toggleFullScreen(_:)), keyEquivalent: "f")
            .keyEquivalentModifierMask = [.control, .command]
        for menu in [app, edit, view] {
            let item = NSMenuItem()
            item.submenu = menu
            main.addItem(item)
        }
        return main
    }
}

/// AppKit moves a window below the menu bar; the strip must sit in that row.
final class StripPanel: NSPanel {
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect { frameRect }
}

let shell = Shell(url: CommandLine.arguments.dropFirst().first.flatMap(URL.init(string:)))
let application = NSApplication.shared
application.delegate = shell
application.setActivationPolicy(.regular)
application.run()
