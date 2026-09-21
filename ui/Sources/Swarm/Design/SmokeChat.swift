#if DEBUG
import AppKit
import Foundation
import SwarmCore

/// `Swarm --smoke-chat` starts a real CLI chat and uses it, the way a person does.
///
/// **Every chat fault this app has shipped got through a green test suite**, because the suite
/// stops at the edge of the process: a fake bus, a fake store, a fake pane. The CLI that never
/// started, the send that the pane refused, the chat that came back as a new conversation and the
/// Codex update question nobody could answer were all found by the owner, one at a time. This is
/// the one check that leaves the process: it launches the CLI, waits for its own words to come
/// back through the provider's log, sends a second message into the running pane, starts the CLI
/// again on the same conversation and asks for the earlier answer back.
///
/// It costs three short turns of a real model, so it is run by hand before an install rather than
/// in the test suite. `Tools/chat-smoke.sh` is what runs it.
enum SmokeChat {
    static var isRequested: Bool { CommandLine.arguments.contains("--smoke-chat") }

    static func runAndExit() -> Never {
        NSApplication.shared.setActivationPolicy(.prohibited)
        Task { await run() }
        RunLoop.main.run()
        exit(1)
    }

    /// How long one answer may take. A first Codex turn on a loaded machine is a few seconds; the
    /// margin is for a cold start and a slow model.
    private static let answerTimeout = Duration.seconds(180)
    private static let startTimeout = Duration.seconds(60)

    private static func argument(_ name: String, or fallback: String) -> String {
        let arguments = CommandLine.arguments
        guard let index = arguments.firstIndex(of: name), index + 1 < arguments.count,
              !arguments[index + 1].hasPrefix("--")
        else { return fallback }
        return arguments[index + 1]
    }

    @MainActor
    private static func run() async -> Never {
        let directory = argument("--smoke-chat", or: FileManager.default.currentDirectoryPath)
        let kind = AgentKind(rawValue: argument("--agent", or: AgentKind.codex.rawValue)) ?? .codex
        let model = argument("--model", or: kind == .codex ? "gpt-5.5" : "opus")
        let run = UUID().uuidString.prefix(8)
        // A real id, because Claude Code takes it as `--session-id` and refuses anything that is
        // not a UUID. A chat row's id is one, so a made-up name here would test a shape the app
        // never uses.
        let sessionID = SessionID(UUID().uuidString)
        let paneID = TerminalTabID("smoke-pane-\(run)")
        let workspaceID = WorkspaceID("smoke-\(run)")
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("swarm-smoke-\(run)", isDirectory: true)

        say("chat smoke, \(kind.label) \(model), in \(directory)")

        guard let swarmSession = await makeSwarmSession(home: home.path) else {
            fail("swarm could not open a session in \(home.path)")
        }

        var session = Session(
            id: sessionID, workspaceID: workspaceID, title: "Smoke",
            model: model, effort: "low", agentKind: kind, permissionMode: .acceptEdits
        )
        let marker = "swarm-smoke-\(run)"

        // 1. It starts, and its first answer arrives.
        var started = Date()
        await launch(
            session: session, paneID: paneID, workspaceID: workspaceID, swarmSession: swarmSession,
            directory: directory, home: home.path, prompt: reply(with: marker + "-one"),
            resuming: nil, replacing: false
        )
        guard let provider = await waitForProviderSession(sessionID, since: started) else {
            fail("the CLI never reported a session id, so it did not start")
        }
        say("started, provider session \(provider)")
        session.agentSessionID = provider
        await expect(marker + "-one", agent: kind, provider: provider, sessionID: sessionID, step: "first answer")

        // 2. A message typed into the running pane reaches it.
        let typed = await TerminalSessionStore.shared.submitToAgent(
            reply(with: marker + "-two"), paneID: paneID.rawValue, workspaceID: workspaceID
        )
        guard typed else { fail("the running pane refused a typed message") }
        await expect(marker + "-two", agent: kind, provider: provider, sessionID: sessionID, step: "typed answer")

        // 3. Started again, it is the same conversation, and it still answers.
        started = Date()
        await launch(
            session: session, paneID: paneID, workspaceID: workspaceID, swarmSession: swarmSession,
            directory: directory, home: home.path, prompt: reply(with: marker + "-three"),
            resuming: provider, replacing: true
        )
        guard let resumed = await waitForProviderSession(sessionID, since: started) else {
            fail("the CLI did not start again")
        }
        say("started again, provider session \(resumed)")
        // The fault the owner reported as "the conversation is created just like that". A restart
        // that resumes keeps the id; a restart that starts fresh takes a new one.
        guard resumed == provider else {
            fail("the restart made a new conversation, \(provider) became \(resumed)")
        }
        await expect(
            marker + "-three", agent: kind, provider: resumed, sessionID: sessionID,
            step: "answer after the restart"
        )
        await expect(
            marker + "-one", agent: kind, provider: resumed, sessionID: sessionID,
            step: "the conversation carried over the restart", ofAgent: false
        )

        await clean(workspaceID: workspaceID, paneID: paneID, home: home)
        say("PASS")
        exit(0)
    }

    /// The prompt. Short, tool-free and exact, so an answer is one line and needs no approval.
    private static func reply(with word: String) -> String {
        "Reply with exactly this word and nothing else: \(word)"
    }

    private static func makeSwarmSession(home: String) async -> SwarmSessionID? {
        guard let swarm = Shell.which("swarm") else { return nil }
        try? FileManager.default.createDirectory(atPath: home, withIntermediateDirectories: true)
        guard let initialised = try? await Shell.run(
            swarm, ["init"], env: ["SWARM_HOME": home], timeout: .seconds(20)
        ), initialised.ok else { return nil }
        guard let made = try? await Shell.run(
            swarm, ["session", "new", "lane"], cwd: home, env: ["SWARM_HOME": home],
            timeout: .seconds(20)
        ), made.ok else { return nil }
        let id = made.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        return id.isEmpty ? nil : SwarmSessionID(id)
    }

    @MainActor
    private static func launch(
        session: Session, paneID: TerminalTabID, workspaceID: WorkspaceID,
        swarmSession: SwarmSessionID, directory: String, home: String, prompt: String,
        resuming: String?, replacing: Bool
    ) async {
        guard let plan = SwarmChairLaunch.plan(
            workspaceID: workspaceID, session: session, paneID: paneID,
            swarmSession: swarmSession, directory: directory, prompt: prompt, home: home,
            workspaceEnvironment: ["PATH": ProcessInfo.processInfo.environment["PATH"] ?? ""],
            resuming: resuming
        ) else { fail("no launch plan for \(session.agentKind.label)") }
        try? FileManager.default.removeItem(at: AgentKind.interactiveStatusURL(sessionID: session.id))
        do {
            try await TerminalSessionStore.shared.launch(plan, replacing: replacing)
        } catch {
            fail("the launch failed: \(error.readableMessage)")
        }
    }

    /// The provider's own session id, which the CLI's `SessionStart` hook writes. A write later
    /// than `since` is the proof that the CLI is up rather than sitting at a question.
    ///
    /// The time rather than a new id, because a resumed conversation keeps the id it had: Codex
    /// answered `codex resume` with the same session id, and a check that waited for a different
    /// one called a healthy restart a failure.
    private static func waitForProviderSession(
        _ sessionID: SessionID, since: Date
    ) async -> String? {
        let url = AgentKind.interactiveStatusURL(sessionID: sessionID)
        let deadline = ContinuousClock.now.advanced(by: startTimeout)
        while ContinuousClock.now < deadline {
            if let written = try? url.resourceValues(forKeys: [.contentModificationDateKey])
                .contentModificationDate, written >= since,
               let data = try? Data(contentsOf: url),
               let id = AgentKind.interactiveHookSessionID(data: data) {
                return id
            }
            try? await Task.sleep(for: .milliseconds(500))
        }
        return nil
    }

    /// Waits for `word` to appear in the conversation Swarm draws, which is the provider's log read
    /// by the same code the chat pane uses.
    private static func expect(
        _ word: String, agent: AgentKind, provider: String, sessionID: SessionID, step: String,
        ofAgent: Bool = true
    ) async {
        let deadline = ContinuousClock.now.advanced(by: answerTimeout)
        while ContinuousClock.now < deadline {
            if case .success(let reader) = InteractiveChatTranscript.reader(
                agent: agent, providerSessionID: provider, sessionID: sessionID
            ), case .success(let transcript) = await InteractiveChatTranscript.read(reader) {
                let rows = ofAgent
                    ? transcript.messages.filter { $0.kind == .assistantText }
                    : transcript.messages
                if rows.contains(where: {
                    String(decoding: $0.payload, as: UTF8.self).contains(word)
                }) {
                    say("ok, \(step)")
                    return
                }
            }
            try? await Task.sleep(for: .seconds(2))
        }
        fail("\(step): \(word) never arrived")
    }

    @MainActor
    private static func clean(workspaceID: WorkspaceID, paneID: TerminalTabID, home: URL) async {
        await TerminalSessionStore.shared.persistence?.kill(
            workspaceID: workspaceID, paneIDs: [paneID.rawValue]
        )
        try? FileManager.default.removeItem(at: home)
    }

    private static func say(_ text: String) {
        print("smoke: " + text)
        fflush(stdout)
    }

    private static func fail(_ text: String) -> Never {
        FileHandle.standardError.write(Data(("smoke: FAIL " + text + "\n").utf8))
        exit(1)
    }
}
#endif
