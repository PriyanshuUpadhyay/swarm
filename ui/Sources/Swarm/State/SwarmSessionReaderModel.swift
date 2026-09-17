import Foundation
import Observation
import SwarmCore

/// The live, read-only state for one discovered swarm session.
@MainActor
@Observable
final class SwarmSessionReaderModel {
    private let session: SwarmSession
    @ObservationIgnored private let bus: any SwarmBus

    private(set) var rows: [TranscriptRow] = []
    private(set) var droppedRows = 0
    private(set) var chatFailure: String?
    private(set) var agents: [SwarmSessionAgentDigest] = []
    private(set) var agentsFailure: String?
    private var inputFailures: [SwarmAgentID: String] = [:]
    private var listedAgents: [SwarmAgentID: SwarmAgent] = [:]

    init(session: SwarmSession, bus: any SwarmBus) {
        self.session = session
        self.bus = bus
        if (try? SwarmSessionInteraction.adapter(for: session)) == nil {
            agentsFailure = SwarmSessionInteraction.missingAdapterSentence
        }
    }

    func follow() async {
        while !Task.isCancelled {
            await refresh()
            do {
                try await Task.sleep(for: .seconds(SubagentPane.refreshSeconds))
            } catch {
                return
            }
        }
    }

    private func refresh() async {
        let chairLog = session.chairLog
        let transcriptID = SessionID("swarm-" + session.id.rawValue)
        async let chat = Task.detached(priority: .utility) {
            Self.readChat(path: chairLog, sessionID: transcriptID)
        }.value
        let reading = await chat
        guard !Task.isCancelled else { return }
        if rows != reading.rows { rows = reading.rows }
        if droppedRows != reading.droppedRows { droppedRows = reading.droppedRows }
        chatFailure = reading.failure

        do {
            _ = try SwarmSessionInteraction.adapter(for: session)
        } catch {
            agents = []
            listedAgents = [:]
            agentsFailure = Self.message(for: error)
            return
        }

        async let freshAgents = bus.agents(in: session)
        async let freshMessages = bus.messages(in: session, after: 0)

        do {
            let (listedAgents, listedMessages) = try await (freshAgents, freshMessages)
            let digest = SwarmSessionAgents.digests(
                agents: listedAgents, messages: listedMessages
            )
            guard !Task.isCancelled else { return }
            if agents != digest { agents = digest }
            self.listedAgents = Dictionary(uniqueKeysWithValues: listedAgents.map { ($0.id, $0) })
            agentsFailure = nil
        } catch is CancellationError {
            return
        } catch {
            agentsFailure = Self.message(for: error)
        }
    }

    func disabledReason(
        for agent: SwarmAgentID, target: SwarmSessionInputTarget
    ) -> String? {
        SwarmSessionInteraction.disabledReason(
            adapter: session.adapter, pane: listedAgents[agent]?.pane, target: target
        )
    }

    func canSubmit(
        _ text: String, to agent: SwarmAgentID, target: SwarmSessionInputTarget
    ) -> Bool {
        SwarmSessionInteraction.canSubmit(
            text, adapter: session.adapter, pane: listedAgents[agent]?.pane, target: target
        )
    }

    func inputFailure(for agent: SwarmAgentID) -> String? {
        inputFailures[agent]
    }

    func type(_ text: String, to agent: SwarmAgentID) async -> Bool {
        do {
            try await bus.type(text, to: agent, in: session)
            inputFailures[agent] = nil
            return true
        } catch {
            inputFailures[agent] = Self.message(for: error)
            return false
        }
    }

    func interrupt(_ agent: SwarmAgentID) async {
        do {
            try await bus.interrupt(agent, in: session)
            inputFailures[agent] = nil
        } catch {
            inputFailures[agent] = Self.message(for: error)
        }
    }

    nonisolated private static func readChat(
        path: String?, sessionID: SessionID
    ) -> ChairReading {
        switch ChairTranscriptOutput.read(path: path, sessionID: sessionID) {
        case .success(let transcript):
            return ChairReading(
                rows: TranscriptModel.rows(from: transcript.messages),
                droppedRows: transcript.droppedRows,
                failure: nil
            )
        case .failure(.noFile):
            return ChairReading(failure: "This session has no chair chat log.")
        case .failure(.missing):
            return ChairReading(failure: "The chair chat log is missing.")
        case .failure(.unreadable(let reason)):
            return ChairReading(failure: "The chair chat log could not be read. \(reason)")
        }
    }

    nonisolated private static func message(for error: any Error) -> String {
        switch error {
        case SwarmProfileError.unavailable(let text), SwarmProfileError.failed(let text): text
        default: error.localizedDescription
        }
    }
}

private struct ChairReading: Sendable {
    var rows: [TranscriptRow] = []
    var droppedRows = 0
    var failure: String?
}
