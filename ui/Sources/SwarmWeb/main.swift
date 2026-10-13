// SwarmWeb: Swarm.app's main executable. One window with one WKWebView on a `swarm serve` child.
// usage: SwarmWeb [--swarm BIN] [--assets DIR] [--app react|solid]
//        In the app bundle, BIN is SWARM_BIN or Contents/Helpers/swarm and DIR is Contents/Resources/web.
//        SwarmWeb ... --smoke
//        SwarmWeb ... --measure OUT.json [--live-seconds N] [--soak-minutes N] [--gen web/perf/gen.mjs]
// It never activates itself: the window comes to the front of its level only (owner rule).
import AppKit
import WebKit

@MainActor
final class Shell: NSObject, NSApplicationDelegate {
    let options: [String: String]
    let webView = WKWebView(frame: .zero)
    var window: NSWindow!
    var children: [Process] = []
    var signals: [DispatchSourceSignal] = []
    /// `http://127.0.0.1:<port>` and `token=<hex>`, from the line that `swarm serve` prints.
    var origin = ""
    var token = ""

    // ⌘N and ⌘T have no command in the pilot web app yet, so `swarmCommand` returns false for them.
    static let commands: [(title: String, id: String, key: String, mods: NSEvent.ModifierFlags)] =
        (1...9).map { (title: $0 == 9 ? "Last Chat" : "Chat \($0)", id: "chat.\($0)", key: "\($0)", mods: .command) } + [
            ("Next Chat", "chat.next", "}", .command),
            ("Previous Chat", "chat.prev", "{", .command),
            ("Command Palette", "palette", "k", .command),
            ("Focus Composer", "composer.focus", "l", .command),
            ("New Workspace", "workspace.new", "n", .command),
            ("New Chat", "chat.new", "t", .command),
            ("Next Workspace", "workspace.next", String(UnicodeScalar(NSDownArrowFunctionKey)!), [.control, .command]),
            ("Previous Workspace", "workspace.prev", String(UnicodeScalar(NSUpArrowFunctionKey)!), [.control, .command]),
        ]

    init(options: [String: String]) {
        self.options = options
    }

    var entry: String { options["app"] == "solid" ? "/solid.html" : "/" }
    var swarm: String {
        options["swarm"] ?? ProcessInfo.processInfo.environment["SWARM_BIN"]
            ?? Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/swarm").path
    }
    var assets: String {
        options["assets"] ?? Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/web").path
    }
    var measuring: Bool { options["measure"] != nil || options["smoke"] != nil }

    func applicationDidFinishLaunching(_ notification: Notification) {
        // SIGINT and SIGTERM quit through AppKit, so the serve child stops too.
        for number in [SIGINT, SIGTERM] {
            signal(number, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: number, queue: .main)
            source.setEventHandler { MainActor.assumeIsolated { NSApp.terminate(nil) } }
            source.resume()
            signals.append(source)
        }
        NSApp.mainMenu = makeMenu()
        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        // A measure run sits in the bottom-right corner at a fixed size, so runs compare.
        let size = measuring ? NSSize(width: 900, height: 600) : NSSize(width: 1280, height: 800)
        window = NSWindow(
            contentRect: NSRect(origin: NSPoint(x: screen.maxX - size.width, y: screen.minY), size: size),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Swarm"
        if !measuring {
            window.center()
            window.setFrameAutosaveName("Swarm")
        }
        window.contentView = webView
        window.makeFirstResponder(webView)
        window.orderFrontRegardless()
        webView.configuration.userContentController.addScriptMessageHandler(self, contentWorld: .page, name: "swarm")
        Task {
            let login = await Task.detached { LoginPath.discover() }.value
            do {
                try startServe(login: login)
                webView.load(URLRequest(url: pageURL()))
            } catch {
                fail("\(error)")
                return
            }
            guard measuring else {
                followBadge()
                await warnPathDrift(login: login)
                return
            }
            do {
                if options["smoke"] != nil { try await smoke() } else { try await measure() }
                quit(0)
            } catch {
                FileHandle.standardError.write(Data("SwarmWeb: \(error)\n".utf8))
                quit(1)
            }
        }
    }

    func pageURL(_ query: String = "") -> URL {
        URL(string: "\(origin)\(entry)\(query)#\(token)")!
    }

    /// Starts `swarm serve` and reads the URL from its first stdout line. `login` is the login
    /// shell's PATH.
    func startServe(login: [String]) throws {
        let serve = Process()
        serve.executableURL = URL(fileURLWithPath: swarm)
        serve.arguments = ["serve", "--port", "0", "--assets", assets]
        // SWARM_HOME passes through. Serve's own transcript default is a path in the build checkout.
        var environment = ProcessInfo.processInfo.environment
        environment["PATH"] = LoginPath.merged(
            login: login, inherited: environment["PATH"] ?? "", home: NSHomeDirectory())
        let transcript = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/transcript").path
        if environment["SWARM_TRANSCRIPT_TOOL", default: ""].isEmpty,
           FileManager.default.isExecutableFile(atPath: transcript) {
            environment["SWARM_TRANSCRIPT_TOOL"] = transcript
        }
        serve.environment = environment
        let stdout = Pipe()
        serve.standardOutput = stdout
        serve.terminationHandler = { process in
            let status = process.terminationStatus
            Task { @MainActor in self.fail("swarm serve stopped (exit \(status)).") }
        }
        try serve.run()
        children.append(serve)
        var line = Data()
        while !line.contains(10) {
            let chunk = stdout.fileHandleForReading.availableData
            if chunk.isEmpty { break }
            line.append(chunk)
        }
        let parts = String(decoding: line, as: UTF8.self).split(separator: "\n").first?.split(separator: "#", maxSplits: 1) ?? []
        guard parts.count == 2, parts[0].hasPrefix("http://127.0.0.1:") else {
            throw ShellError("swarm serve printed no URL.")
        }
        origin = String(parts[0].hasSuffix("/") ? parts[0].dropLast() : parts[0])
        token = String(parts[1])
    }

    /// Shows one line of error in the window. A measure or smoke run stops instead.
    func fail(_ message: String) {
        if measuring {
            FileHandle.standardError.write(Data("SwarmWeb: \(message)\n".utf8))
            quit(1)
        }
        let text = message.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;")
        webView.loadHTMLString("<p style=\"font: 13px -apple-system; padding: 16px\">\(text)</p>", baseURL: nil)
    }

    func makeMenu() -> NSMenu {
        let main = NSMenu()
        let app = NSMenu()
        app.addItem(withTitle: "Hide Swarm", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        app.addItem(withTitle: "Quit Swarm", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        // The web view's text fields get ⌘C, ⌘V and the rest only through these menu items.
        let edit = NSMenu(title: "Edit")
        for (title, action, key, mods) in [
            ("Undo", "undo:", "z", NSEvent.ModifierFlags.command), ("Redo", "redo:", "z", [.command, .shift]),
            ("Cut", "cut:", "x", .command), ("Copy", "copy:", "c", .command), ("Paste", "paste:", "v", .command),
            ("Select All", "selectAll:", "a", .command),
        ] {
            edit.addItem(withTitle: title, action: Selector(action), keyEquivalent: key).keyEquivalentModifierMask = mods
        }
        let go = NSMenu(title: "Go")
        for command in Self.commands {
            let item = NSMenuItem(title: command.title, action: #selector(runCommand(_:)), keyEquivalent: command.key)
            item.keyEquivalentModifierMask = command.mods
            item.representedObject = command.id
            item.target = self
            go.addItem(item)
        }
        for menu in [app, edit, go] {
            let item = NSMenuItem()
            item.submenu = menu
            main.addItem(item)
        }
        return main
    }

    /// Every menu item calls the page's one entry point. The ids are fixed strings from `commands`.
    @objc func runCommand(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? String else { return }
        webView.evaluateJavaScript("window.swarmCommand?.('\(id)')")
    }

    func stopChildren() {
        for child in children where child.isRunning {
            child.terminationHandler = nil
            child.terminate()
        }
    }

    func quit(_ code: Int32) -> Never {
        stopChildren()
        exit(code)
    }

    func applicationWillTerminate(_ notification: Notification) {
        stopChildren()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

/// The page's one script message (ADR 0077). Only `{"kind": "pickFolder"}` is valid.
enum ShellMessage: Equatable {
    case pickFolder

    init?(body: Any) {
        guard let body = body as? [String: Any], body.count == 1,
              body["kind"] as? String == "pickFolder" else { return nil }
        self = .pickFolder
    }
}

extension Shell: WKScriptMessageHandlerWithReply {
    /// Replies with the picked folder's path, or null when the owner cancels.
    func userContentController(_ controller: WKUserContentController,
                               didReceive message: WKScriptMessage) async -> (Any?, String?) {
        let page = message.frameInfo.securityOrigin
        guard message.frameInfo.isMainFrame, "http://\(page.host):\(page.port)" == origin,
              ShellMessage(body: message.body) == .pickFolder else {
            return (nil, "swarm: unknown shell message")
        }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        let response = await panel.beginSheetModal(for: window)
        return (response == .OK ? panel.url?.path : nil, nil)
    }
}

extension Shell {
    /// One alert per launch when Terminal's `swarm` is another build (ADR 0048). Copy Command and
    /// Not Now both store the pair, so it never asks again.
    func warnPathDrift(login: [String]) async {
        let key = "dismissedPathSwarms"
        let dismissed = UserDefaults.standard.stringArray(forKey: key) ?? []
        let branch = Bundle.main.object(forInfoDictionaryKey: "SwarmBuildBranch") as? String ?? ""
        let swarm = swarm
        guard let drift = await Task.detached(operation: {
            PathDrift.check(branch: branch, loginPath: login, helper: swarm, dismissed: dismissed)
        }).value else { return }
        let alert = NSAlert()
        alert.messageText = "Terminal runs another swarm"
        alert.informativeText = drift.message
        alert.addButton(withTitle: "Copy Command")
        alert.addButton(withTitle: "Not Now")
        let response = await alert.beginSheetModal(for: window)
        UserDefaults.standard.set(dismissed + [drift.key], forKey: key)
        if response == .alertFirstButtonReturn {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(drift.fixCommand, forType: .string)
        }
    }

    /// Keeps the Dock badge on `/api/badge`, read again on each snapshot frame of `/api/stream`.
    /// It stops when `swarm serve` stops.
    func followBadge() {
        Task {
            while children.first?.isRunning == true {
                do {
                    let (lines, _) = try await URLSession.shared.bytes(for: request("/api/stream"))
                    for try await line in lines.lines where line == "event: hello" || line == "event: snapshot" {
                        let (data, _) = try await URLSession.shared.data(for: request("/api/badge"))
                        let count = (try JSONSerialization.jsonObject(with: data) as? [String: Any])?["count"] as? Int ?? 0
                        NSApp.dockTile.badgeLabel = count == 0 ? nil : String(count)
                    }
                } catch {
                    FileHandle.standardError.write(Data("SwarmWeb: badge: \(error)\n".utf8))
                }
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    func request(_ path: String) -> URLRequest {
        var request = URLRequest(url: URL(string: "\(origin)\(path)")!)
        // `token` is the page fragment, `token=<hex>`; the API takes the hex alone.
        request.setValue("Bearer \(token.replacingOccurrences(of: "token=", with: ""))", forHTTPHeaderField: "Authorization")
        return request
    }
}

struct ShellError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

var options: [String: String] = [:]
var arguments = CommandLine.arguments.dropFirst()
while let flag = arguments.popFirst() {
    // A flag with no value after it, such as --smoke, is a switch.
    let value = arguments.first.map { $0.hasPrefix("--") ? nil : $0 } ?? nil
    if value != nil { arguments.removeFirst() }
    options[String(flag.drop(while: { $0 == "-" }))] = value ?? "true"
}

let shell = Shell(options: options)
let application = NSApplication.shared
application.delegate = shell
// A menu bar needs the regular policy. Launch does not activate the app; a click on the window does.
application.setActivationPolicy(.regular)
application.run()
