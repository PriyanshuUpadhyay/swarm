import Foundation
import Observation
import BloomCore

/// Live swarm state for one workspace, including the one polling task its agent tabs share.
@MainActor
@Observable
final class SwarmAgentWorkspaceModel {
    private let workspaceID: WorkspaceID
    private let directory: String
    @ObservationIgnored private let bus: any SwarmBus
    @ObservationIgnored private let store: Store?

    private(set) var sessionID: SwarmSessionID?
    private(set) var agents: [SwarmAgentID: SwarmAgent] = [:]
    private(set) var messages: [SwarmMessage] = []
    private(set) var lastError: String?
    private var pendingComposers: [SwarmAgentID: String] = [:]

    @ObservationIgnored private var pollTask: Task<Void, Never>?
    @ObservationIgnored private var trackedAgents: Set<SwarmAgentID> = []
    @ObservationIgnored private var visibleAgents: Set<SwarmAgentID> = []
    @ObservationIgnored private var lastSeq = 0
    @ObservationIgnored private var failureCount = 0
    @ObservationIgnored private var lastSweep: Date?
    @ObservationIgnored private var acknowledging: Set<Int> = []
    @ObservationIgnored private var dismissedError: String?

    init(
        workspaceID: WorkspaceID, directory: String,
        bus: any SwarmBus, store: Store?
    ) {
        self.workspaceID = workspaceID
        self.directory = directory
        self.bus = bus
        self.store = store
    }

    func sync(agentIDs: [SwarmAgentID]) {
        trackedAgents = Set(agentIDs)
        visibleAgents.formIntersection(trackedAgents)
        guard !trackedAgents.isEmpty else {
            pollTask?.cancel()
            pollTask = nil
            return
        }
        guard pollTask == nil else { return }
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let delay = await self.poll()
                do {
                    try await Task.sleep(for: .seconds(delay))
                } catch {
                    return
                }
            }
        }
    }

    func start(
        agent: SwarmAgentID, role: String, account: String?, firstMessage: String
    ) async throws -> SwarmSessionID {
        guard let store else { throw SwarmAgentWorkspaceError.storeUnavailable }
        let session: SwarmSessionID
        let saved = await SwarmWorkspaceSession.load(workspaceID: workspaceID, from: store)
        if let existing = sessionID ?? saved {
            session = existing
        } else {
            session = try await bus.startSession()
            try await SwarmWorkspaceSession.save(session, workspaceID: workspaceID, in: store)
        }
        sessionID = session

        let launch = try await bus.launch(
            agent, role: role, account: account, in: session, directory: directory
        )
        agents[agent] = SwarmAgent(id: agent, role: role, pane: launch.pane, alive: true)
        do {
            let seq = try await bus.send(firstMessage, to: agent, in: session)
            merge([outgoingMessage(seq: seq, body: firstMessage, agent: agent)])
            recordSuccess()
        } catch {
            pendingComposers[agent] = firstMessage
            record(error)
        }
        return session
    }

    /// Refreshes ids before the start sheet suggests a name. No saved session is a successful
    /// empty refresh for a workspace that has not started a swarm yet.
    func refreshAgents() async throws {
        guard let store else { throw SwarmAgentWorkspaceError.storeUnavailable }
        let saved = await SwarmWorkspaceSession.load(workspaceID: workspaceID, from: store)
        guard let session = sessionID ?? saved else {
            recordSuccess()
            return
        }
        sessionID = session
        let fresh = try await bus.agents(in: session)
        agents = Dictionary(uniqueKeysWithValues: fresh.map { ($0.id, $0) })
        recordSuccess()
    }

    func send(_ body: String, to agent: SwarmAgentID) async -> Bool {
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, let session = await loadSession() else { return false }
        do {
            let seq = try await bus.send(text, to: agent, in: session)
            merge([outgoingMessage(seq: seq, body: text, agent: agent)])
            pendingComposers[agent] = nil
            recordSuccess()
            return true
        } catch {
            record(error)
            return false
        }
    }

    func rows(for agent: SwarmAgentID) -> [SwarmChatRow] {
        SwarmChatRow.rows(for: agent, in: messages)
    }

    func show(_ agent: SwarmAgentID) {
        visibleAgents.insert(agent)
        Task { await acknowledgeVisibleMessages() }
    }

    func hide(_ agent: SwarmAgentID) {
        visibleAgents.remove(agent)
    }

    func dismissError() {
        dismissedError = lastError
        lastError = nil
    }

    func pendingComposer(for agent: SwarmAgentID) -> String? {
        pendingComposers[agent]
    }

    /// Archive cleanup is best effort. One failed close is recorded and does not keep the other
    /// agents, or the archive, from continuing.
    func closeAll() async {
        guard let store else {
            record(SwarmAgentWorkspaceError.storeUnavailable)
            return
        }
        let saved = await SwarmWorkspaceSession.load(workspaceID: workspaceID, from: store)
        guard let session = sessionID ?? saved else { return }
        sessionID = session
        do {
            let known = try await bus.agents(in: session)
            agents = Dictionary(uniqueKeysWithValues: known.map { ($0.id, $0) })
            for agent in known where agent.pane != nil {
                do {
                    try await bus.close(agent.id, in: session)
                    agents[agent.id] = SwarmAgent(
                        id: agent.id, role: agent.role, pane: nil, alive: nil
                    )
                } catch {
                    record(error)
                }
            }
        } catch {
            record(error)
        }
    }

    /// Returns false only when a live pane could not be closed, so its tab remains reachable.
    func closeIfNeeded(_ agent: SwarmAgentID) async -> Bool {
        guard let session = await loadSession() else { return true }
        do {
            let known = try await bus.agents(in: session)
            agents = Dictionary(uniqueKeysWithValues: known.map { ($0.id, $0) })
            guard known.first(where: { $0.id == agent })?.pane != nil else { return true }
            try await bus.close(agent, in: session)
            agents[agent] = known.first(where: { $0.id == agent }).map {
                SwarmAgent(id: $0.id, role: $0.role, pane: nil, alive: nil)
            }
            recordSuccess()
            return true
        } catch {
            record(error)
            return false
        }
    }

    private func poll() async -> TimeInterval {
        guard let session = await loadSession() else {
            failureCount += 1
            return SwarmPollSchedule.delay(afterFailures: failureCount)
        }
        do {
            let freshMessages = try await bus.messages(in: session, after: lastSeq)
            let freshAgents = try await bus.agents(in: session)
            merge(freshMessages)
            if let seq = freshMessages.last?.seq { lastSeq = max(lastSeq, seq) }
            agents = Dictionary(uniqueKeysWithValues: freshAgents.map { ($0.id, $0) })
            await acknowledgeVisibleMessages()

            let now = Date()
            let hasPane = freshAgents.contains { $0.pane != nil }
            if SwarmPollSchedule.shouldSweep(last: lastSweep, now: now, hasPane: hasPane) {
                try await bus.sweep(in: session)
                lastSweep = now
            }
            failureCount = 0
            recordSuccess()
        } catch {
            failureCount += 1
            record(error)
        }
        return SwarmPollSchedule.delay(afterFailures: failureCount)
    }

    private func loadSession() async -> SwarmSessionID? {
        if let sessionID { return sessionID }
        guard let store else {
            record(SwarmAgentWorkspaceError.storeUnavailable)
            return nil
        }
        sessionID = await SwarmWorkspaceSession.load(workspaceID: workspaceID, from: store)
        if sessionID == nil { record(SwarmAgentWorkspaceError.sessionUnavailable) }
        return sessionID
    }

    private func acknowledgeVisibleMessages() async {
        guard let sessionID else { return }
        let sequences = SwarmChatRow.acknowledgements(
            in: messages, shownAgents: visibleAgents
        ).filter { !acknowledging.contains($0) }
        for seq in sequences {
            acknowledging.insert(seq)
            do {
                try await bus.ack(seq, in: sessionID)
                if let index = messages.firstIndex(where: { $0.seq == seq }) {
                    messages[index].read = true
                }
                acknowledging.remove(seq)
            } catch {
                acknowledging.remove(seq)
                record(error)
                return
            }
        }
    }

    private func merge(_ additions: [SwarmMessage]) {
        guard !additions.isEmpty else { return }
        var bySequence = Dictionary(uniqueKeysWithValues: messages.map { ($0.seq, $0) })
        for message in additions { bySequence[message.seq] = message }
        messages = bySequence.values.sorted { $0.seq < $1.seq }
    }

    private func record(_ error: any Error) {
        let message: String
        switch error {
        case SwarmProfileError.unavailable(let text), SwarmProfileError.failed(let text):
            message = text
        default:
            message = error.localizedDescription
        }
        guard dismissedError != message, lastError != message else { return }
        lastError = message
    }

    private func recordSuccess() {
        dismissedError = nil
    }

    private func outgoingMessage(
        seq: Int, body: String, agent: SwarmAgentID
    ) -> SwarmMessage {
        SwarmMessage(
            seq: seq,
            sender: SwarmAgentID("orchestrator"),
            recipient: agent,
            kind: "ask",
            body: body,
            createdAt: Int(Date().timeIntervalSince1970),
            read: false
        )
    }
}

private enum SwarmAgentWorkspaceError: LocalizedError {
    case storeUnavailable
    case sessionUnavailable

    var errorDescription: String? {
        switch self {
        case .storeUnavailable:
            "Bloom has not finished opening its store, so it cannot save this swarm session."
        case .sessionUnavailable:
            "This workspace has no saved swarm session."
        }
    }
}
