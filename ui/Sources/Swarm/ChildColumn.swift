import Observation
import SwiftUI
import SwarmCore

/// One child agent's chat: its transcript from the log its hooks reported, a draft, and the calls
/// that type to it, stop it, and answer its questions (ADR 0029).
@MainActor @Observable
final class ChildColumnModel {
    private let transcript = SwarmChairTranscript()
    private let bus = SwarmCLIBus()
    private(set) var snapshot: ChairTranscriptSnapshot = .waiting
    private(set) var revision = 0
    private(set) var hasOlder = false
    private(set) var isLoadingOlder = false
    private(set) var historyError: String?
    private(set) var isSending = false
    private(set) var queued: [ComposerQueuedRow] = []
    private var sentMessages = ComposerSentMessages()
    var draft = ""

    private var rows: [TranscriptRow] {
        if case .rows(let rows, _) = snapshot { return rows }
        return []
    }

    /// Reads the child's log once a second while its column is on screen.
    func poll(log: String?, provider: String?) async {
        while !Task.isCancelled {
            let next = await transcript.poll(childLog: log, provider: provider)
            guard !Task.isCancelled else { break }
            hasOlder = await transcript.hasOlder
            if next != snapshot {
                snapshot = next
                revision += 1
            }
            sentMessages.confirm(by: rows)
            let nextQueued = ComposerQueuedRow.queued(await transcript.queuedMessages) + sentMessages.rows
            if queued != nextQueued { queued = nextQueued }
            try? await Task.sleep(for: .seconds(1))
        }
    }

    func loadOlder() async {
        guard hasOlder, !isLoadingOlder else { return }
        isLoadingOlder = true
        historyError = nil
        defer { isLoadingOlder = false }
        do {
            snapshot = try await transcript.loadOlder()
            revision += 1
        } catch {
            historyError = String(describing: error)
        }
        hasOlder = await transcript.hasOlder
    }

    func send(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSession,
        provider: String?, isRunning: Bool
    ) async throws {
        isSending = true
        defer { isSending = false }
        let before = rows
        try await bus.type(text, to: agent, in: session)
        sentMessages.record(text, provider: provider, isRunning: isRunning, transcript: before)
        queued = queued.filter { $0.state == .queued } + sentMessages.rows
        if draft == text { draft = "" }
    }

    func update(isRunning: Bool) {
        sentMessages.update(isRunning: isRunning)
        queued = queued.filter { $0.state == .queued } + sentMessages.rows
    }

    func interrupt(_ agent: SwarmAgentID, in session: SwarmSession) async throws {
        try await bus.interrupt(agent, in: session)
    }

    /// Claude's queued messages back out of the child's CLI; nil when the CLI took them first.
    func pullBack(_ agent: SwarmAgentID, in session: SwarmSession) async throws -> String? {
        let bus = bus
        let text = try await transcript.pullBack { key in
            try await bus.pressKey(key, agent: agent, session: session)
        }
        queued.removeAll { $0.state == .queued }
        return text
    }

    func answer(
        _ prompt: SwarmPrompt, choice: Int, to agent: SwarmAgentID, in session: SwarmSession
    ) async throws {
        try await bus.answer(prompt, choice: choice, to: agent, in: session)
    }

    /// What an empty column says before the child's hooks report its log.
    static func waitingMessage(provider: String?) -> String {
        switch provider {
        case "codex", "agy":
            "No chat yet. If this stays, set up swarm's hooks in Settings, so Codex and AGY report their chats."
        default:
            "No chat yet. The agent reports it on its first turn."
        }
    }
}

/// A child agent's column: its chat as rows, its question when it waits, and a composer.
struct ChildColumnView: View {
    let session: SwarmSession
    let agent: SwarmAgent
    let model: ChildColumnModel
    /// The column has focus, as by a click on it or its Find field.
    let selected: Bool
    /// New on each request for the column, so asking again for the column that has focus still
    /// moves the keyboard to its composer. A click selects the column and keeps the keyboard
    /// where it landed.
    let focusRequest: Int
    let onFocused: () -> Void

    @FocusState private var composerFocused: Bool
    @FocusState private var transcriptFocused: Bool

    var body: some View {
        TranscriptView(
            snapshot: model.snapshot, revision: model.revision,
            hasOlder: model.hasOlder, isLoadingOlder: model.isLoadingOlder,
            historyError: model.historyError,
            waitingMessage: ChildColumnModel.waitingMessage(provider: agent.provider),
            chair: agent.provider, rawSessionJSON: "",
            // Find goes to the focused column; the scene's other key actions stay with the chair.
            isActive: true, isVisible: selected,
            loadOlder: { [model] in await model.loadOlder() },
            onTap: onFocused,
            focus: $transcriptFocused
        ) {
            VStack(spacing: DesignTokens.Spacing.s) {
                if let prompt = agent.prompt {
                    PromptCard(agent: agent.id.rawValue, prompt: prompt) { [model, session, agent] choice in
                        try await model.answer(prompt, choice: choice, to: agent.id, in: session)
                    }
                } else if agent.status == .waiting {
                    Text("\(agent.id.rawValue) waits on a question the app cannot read. Run `swarm attach \(agent.id.rawValue)` in a terminal to answer it.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                composer
            }
            .animation(.easeOut(duration: 0.15), value: agent.prompt?.id)
        }
        .task(id: (agent.log ?? "") + (agent.provider ?? "")) {
            await model.poll(log: agent.log, provider: agent.provider)
        }
        .onChange(of: focusRequest, initial: true) {
            if selected { composerFocused = true }
        }
        .onChange(of: agent.status == .working) { _, running in model.update(isRunning: running) }
    }

    private var composer: some View {
        ComposerView(
            sessionID: session.id.rawValue + ":" + agent.id.rawValue,
            draft: Binding(get: { [model] in model.draft }, set: { [model] in model.draft = $0 }),
            isRunning: agent.status == .working,
            isSending: model.isSending,
            queued: model.queued,
            pullBack: SwarmSessionInteraction.canPullBack(provider: agent.provider, adapter: session.adapter)
                ? { [model, session, agent] in try await model.pullBack(agent.id, in: session) }
                : nil,
            sendDisabledReason: agent.alive == false ? "This agent has ended." : nil,
            placeholder: "Message \(agent.id.rawValue)",
            commandSource: ComposerCommandSource(
                provider: agent.provider,
                homeDirectory: FileManager.default.homeDirectoryForCurrentUser.path,
                configDirectory: agent.log.flatMap {
                    ComposerCommandSource.configDirectory(fromLog: $0, provider: agent.provider)
                },
                projectDirectory: session.cwd
            ),
            mentionSource: ComposerMentionSource(root: session.cwd),
            scratchDirectory: AgentScratchDirectory.current(),
            focus: $composerFocused,
            send: { [model, session, agent] in
                try await model.send(
                    $0, to: agent.id, in: session,
                    provider: agent.provider, isRunning: agent.status == .working
                )
            },
            interrupt: { [model, session, agent] in try await model.interrupt(agent.id, in: session) },
            onFocused: onFocused,
            isCurrentSession: { true }
        )
    }
}
