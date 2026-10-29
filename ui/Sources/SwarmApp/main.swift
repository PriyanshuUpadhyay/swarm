// SwarmApp: Swarm.app's executable, one window with one WKWebView on the `swarm ui`
// server. `swarm ui` starts it with `open -a Swarm.app --args <url>`; when it already runs, that
// `open` only activates it and its window comes to the front. A copy at another path, such as the
// one of a newer Homebrew version, starts a second process, which closes the older one.
// usage: SwarmApp <http://127.0.0.1:PORT/#token=HEX>
import AppKit
import WebKit

@MainActor
final class Shell: NSObject, NSApplicationDelegate, NSWindowDelegate, WKUIDelegate, WKNavigationDelegate, WKScriptMessageHandlerWithReply {
    var url: URL?
    let webView = WKWebView(frame: .zero)
    var window: NSWindow!
    /// Paints the camera row above the full-screen window in the page's background colour. A
    /// full-screen window never covers that row; macOS leaves it black.
    var strip: NSPanel?
    var stripColor: NSKeyValueObservation?
    /// The owner's View › Zoom level, kept in this app's UserDefaults. A browser tab has its own zoom.
    static let lastURLKey = "lastServerURL"
    static let zoomKey = "pageZoom"
    /// Whether the window was in full screen when the app last quit, so a relaunch goes back to it.
    static let fullScreenKey = "fullScreen"

    init(url: URL?) {
        self.url = url ?? UserDefaults.standard.url(forKey: Self.lastURLKey)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        // An upgrade moves the bundle to a new versioned path, so `open -a` starts this process
        // beside the old one instead of activating it. The newest launch keeps the one window.
        if let id = Bundle.main.bundleIdentifier {
            for other in NSRunningApplication.runningApplications(withBundleIdentifier: id) where other != .current {
                other.terminate()
            }
        }
        NSApp.mainMenu = makeMenu()
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Swarm"
        window.center()
        // AppKit saves the frame on each move or resize and puts it back here, also a zoomed one.
        window.setFrameAutosaveName("Swarm")
        window.collectionBehavior = .fullScreenPrimary
        window.delegate = self
        window.backgroundColor = NSColor(red: 0x10 / 255, green: 0x10 / 255, blue: 0x10 / 255, alpha: 1)
        webView.uiDelegate = self
        webView.navigationDelegate = self
        webView.allowsBackForwardNavigationGestures = true
        webView.configuration.userContentController.addScriptMessageHandler(self, contentWorld: .page, name: "swarm")
        let zoom = UserDefaults.standard.double(forKey: Self.zoomKey)
        if zoom > 0 { setZoom(zoom) }
        window.contentView = webView
        window.makeFirstResponder(webView)
        window.makeKeyAndOrderFront(nil)
        if UserDefaults.standard.bool(forKey: Self.fullScreenKey) { window.toggleFullScreen(nil) }
        if let url, url.host == "127.0.0.1", url.scheme == "http" {
            UserDefaults.standard.set(url, forKey: Self.lastURLKey)
            webView.load(URLRequest(url: url))
        } else {
            webView.loadHTMLString("<p style=\"font: 13px -apple-system; padding: 16px\">Run <code>swarm ui</code> to open Swarm.</p>", baseURL: nil)
        }
        NSApp.activate()
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
        UserDefaults.standard.set(true, forKey: Self.fullScreenKey)
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
        UserDefaults.standard.set(false, forKey: Self.fullScreenKey)
        stripColor = nil
        if let strip { window.removeChildWindow(strip); strip.close() }
        strip = nil
    }

    @objc func showSettings(_ sender: Any?) { webView.evaluateJavaScript("window.swarmCommand?.('settings')") }
    @objc func reloadPage(_ sender: Any?) {
        if webView.url?.host == "127.0.0.1" { webView.reload() }
        else if let url { webView.load(URLRequest(url: url)) }
    }
    @objc func goBack(_ sender: Any?) { webView.goBack() }
    @objc func goForward(_ sender: Any?) { webView.goForward() }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        showLoadFailure(error)
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        showLoadFailure(error)
    }

    func showLoadFailure(_ error: Error) {
        guard (error as NSError).code != NSURLErrorCancelled else { return }
        webView.loadHTMLString("""
            <meta name="color-scheme" content="light dark">
            <main style="font: 15px -apple-system; padding: 32px">
            <h1>Could not connect to Swarm</h1>
            <p>Check that the server is running, then try again.</p>
            <button onclick="location.href='swarm-retry:'">Retry</button></main>
            """, baseURL: nil)
    }

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                 decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
        if navigationAction.request.url?.scheme == "swarm-retry" {
            decisionHandler(.cancel)
            reloadPage(nil)
        } else {
            decisionHandler(.allow)
        }
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage,
                               replyHandler: @escaping @MainActor @Sendable (Any?, String?) -> Void) {
        guard message.frameInfo.isMainFrame,
              message.frameInfo.securityOrigin.host == "127.0.0.1",
              message.frameInfo.securityOrigin.protocol == "http",
              message.frameInfo.securityOrigin.port == url?.port,
              let body = message.body as? [String: String], body["kind"] == "pickFolder" else {
            replyHandler(nil, "Unsupported message")
            return
        }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.beginSheetModal(for: window) { response in
            replyHandler(response == .OK ? panel.url?.path : nil, nil)
        }
    }

    @objc func zoomIn(_ sender: Any?) { setZoom(webView.pageZoom + 0.1) }
    @objc func zoomOut(_ sender: Any?) { setZoom(webView.pageZoom - 0.1) }
    @objc func actualSize(_ sender: Any?) { setZoom(1) }

    /// Rounds to a 10% step and keeps 50% to 300%.
    func setZoom(_ zoom: CGFloat) {
        webView.pageZoom = min(3, max(0.5, (zoom * 10).rounded() / 10))
        UserDefaults.standard.set(Double(webView.pageZoom), forKey: Self.zoomKey)
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
        app.addItem(withTitle: "Settings…", action: #selector(showSettings(_:)), keyEquivalent: ",").target = self
        app.addItem(.separator())
        app.addItem(withTitle: "Hide Swarm", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        app.addItem(withTitle: "Hide Others", action: #selector(NSApplication.hideOtherApplications(_:)), keyEquivalent: "h").keyEquivalentModifierMask = [.option, .command]
        app.addItem(withTitle: "Show All", action: #selector(NSApplication.unhideAllApplications(_:)), keyEquivalent: "")
        app.addItem(.separator())
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
        let go = NSMenu(title: "Go")
        go.addItem(withTitle: "Back", action: #selector(goBack(_:)), keyEquivalent: "[").target = self
        go.addItem(withTitle: "Forward", action: #selector(goForward(_:)), keyEquivalent: "]").target = self
        let windows = NSMenu(title: "Window")
        windows.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windows.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        NSApp.windowsMenu = windows
        let view = NSMenu(title: "View")
        view.addItem(withTitle: "Reload", action: #selector(reloadPage(_:)), keyEquivalent: "r").target = self
        view.addItem(.separator())
        view.addItem(withTitle: "Toggle Full Screen", action: #selector(NSWindow.toggleFullScreen(_:)), keyEquivalent: "f")
            .keyEquivalentModifierMask = [.control, .command]
        view.addItem(.separator())
        // ⌘+ shows in the menu; the hidden ⌘= twin takes the same key without Shift, as in Safari.
        for (title, action, key, hidden) in [
            ("Zoom In", #selector(zoomIn(_:)), "+", false), ("Zoom In", #selector(zoomIn(_:)), "=", true),
            ("Zoom Out", #selector(zoomOut(_:)), "-", false), ("Actual Size", #selector(actualSize(_:)), "0", false),
        ] {
            let item = view.addItem(withTitle: title, action: action, keyEquivalent: key)
            item.target = self
            item.isHidden = hidden
            item.allowsKeyEquivalentWhenHidden = hidden
        }
        for menu in [app, edit, view, go, windows] {
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
