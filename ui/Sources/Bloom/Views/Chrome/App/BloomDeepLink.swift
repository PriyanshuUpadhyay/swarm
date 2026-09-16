import AppKit
import Foundation
import BloomCore

/// Preserves Conductor deep links so existing scripts can hand work to Bloom unchanged.
@MainActor
enum BloomDeepLink {
    /// The schemes this copy actually answers to, read off its own bundle rather than written out.
    ///
    /// It was the literal `"bloom"`, and `Tools/dev-build.sh` rebrands the registered scheme to
    /// `bloomdev` so the dev copy cannot swallow a link meant for the real one. The result was a
    /// dev build that registered `bloomdev://`, received them, and then refused every one with
    /// "The link must include a prompt and project path", which is not what was wrong with it.
    /// Reading `CFBundleURLTypes` means the check can only ever disagree with LaunchServices if
    /// the plist does.
    static let schemes: Set<String> = {
        let types = Bundle.main.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]]
        let registered = (types ?? [])
            .compactMap { $0["CFBundleURLSchemes"] as? [String] }
            .flatMap { $0 }
            .map { $0.lowercased() }

        // A bundle with no URL types is `swift run` or a test host. Falling back to the shipped
        // scheme keeps those working rather than making every link fail in a way that reads as a
        // malformed link.
        return registered.isEmpty ? ["bloom"] : Set(registered)
    }()

    /// The Apple Event handler and SwiftUI's onOpenURL can both see the same link, and creating
    /// the same workspace twice is a lot more annoying than dropping a genuine duplicate that
    /// arrived within a second of the first.
    private static var lastHandled: (url: URL, at: Date)?

    static func open(_ url: URL, in app: AppModel) {
        if let last = lastHandled, last.url == url, Date.now.timeIntervalSince(last.at) < 2 {
            return
        }
        lastHandled = (url, .now)

        guard let scheme = url.scheme?.lowercased(), Self.schemes.contains(scheme),
              let values = values(from: url),
              let prompt = values["prompt"]?.removingPercentEncoding,
              let path = values["path"]?.removingPercentEncoding,
              !prompt.isEmpty,
              !path.isEmpty else {
            app.alert = BloomAlert(
                title: "Could not open the Bloom link",
                message: "The link must include a prompt and project path."
            )
            return
        }

        // The same answer `workspace_start` gives to "which project is this path", rather than a
        // second canonicaliser beside it. See `BridgeProjectLookup.project(atPath:in:)`.
        guard let repo = BridgeProjectLookup.project(atPath: path, in: app.repos) else {
            app.alert = BloomAlert(
                title: "Project not found",
                message: "The path in this link is not one of Bloom's projects: \(path)"
            )
            return
        }

        confirmStart(prompt: prompt, in: repo) {
            Task { await app.createWorkspace(in: repo, prompt: prompt) }
        }
    }

    /// Asks before a link starts an agent. A link is outside input: any page or script that can
    /// hand macOS a `bloom://` URL could otherwise create a workspace and run its prompt, and a new
    /// session defaults to full access (`AppDefaults.fallbackPermissionMode`). The prompt is shown
    /// so the reader approves the words the agent will act on, not only the project.
    ///
    /// A sheet on the window rather than `runModal()`, for the reasons `askBeforeQuitting` in
    /// `BloomAppDelegate` gives.
    private static func confirmStart(
        prompt: String, in repo: Repo, then start: @escaping @MainActor @Sendable () -> Void
    ) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "Start a workspace from a link?"
        let shown = prompt.count > 600 ? String(prompt.prefix(600)) + "\u{2026}" : prompt
        alert.informativeText = "Project: \(repo.name)\n\n\(shown)"
        alert.addButton(withTitle: "Start")
        alert.addButton(withTitle: "Cancel")
        // Return cancels, so a link is approved by a deliberate click and not by a key already down.
        alert.buttons.last?.keyEquivalent = "\r"
        alert.buttons.first?.keyEquivalent = ""

        guard let window = NSApp.keyWindow ?? NSApp.mainWindow ?? NSApp.windows.first(where: \.isVisible) else {
            NSApp.activate()
            if alert.runModal() == .alertFirstButtonReturn { start() }
            return
        }

        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        alert.beginSheetModal(for: window) { response in
            MainActor.assumeIsolated {
                if response == .alertFirstButtonReturn { start() }
            }
        }
    }

    private static func values(from url: URL) -> [String: String]? {
        let absolute = url.absoluteString
        guard let separator = absolute.range(of: "://") else { return nil }
        var payload = String(absolute[separator.upperBound...])
        if payload.hasPrefix("?") { payload.removeFirst() }

        var values: [String: String] = [:]
        for pair in payload.split(separator: "&", omittingEmptySubsequences: true) {
            let pieces = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            guard pieces.count == 2 else { continue }
            values[String(pieces[0])] = String(pieces[1]).replacing("+", with: " ")
        }
        return values
    }

}
