import Foundation
import Observation
import SwarmCore

/// The live, read-only state for one discovered swarm session.
@MainActor
@Observable
final class SwarmSessionReaderModel {
    private let sessions: [SwarmSession]
    @ObservationIgnored private let bus: any SwarmBus
    @ObservationIgnored private let chatReader: TranscriptLogReader?

    private(set) var rows: [TranscriptRow] = []
    private(set) var droppedRows = 0
    private(set) var chatFailure: String?
    private(set) var agents: [SwarmSessionAgentDigest] = []
    private(set) var agentsFailure: String?
    private var inputFailures: [InputRoute: String] = [:]
    private var listedAgents: [InputRoute: SwarmAgent] = [:]
    private var chairRoute: InputRoute?

    init(item: SwarmProjectSession, bus: any SwarmBus) {
        self.sessions = item.sessions
        self.bus = bus
        self.chatReader = ChairTranscriptOutput.reader(
            path: item.session.chairLog,
            sessionID: SessionID("swarm-" + item.session.id.rawValue)
        )
        if item.sessions.allSatisfy({ (try? SwarmSessionInteraction.adapter(for: $0)) == nil }) {
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
        let reading = await Self.readChat(chatReader)
        guard !Task.isCancelled else { return }
        if rows != reading.rows { rows = reading.rows }
        if droppedRows != reading.droppedRows { droppedRows = reading.droppedRows }
        chatFailure = reading.failure

        do {
            for session in sessions {
                _ = try SwarmSessionInteraction.adapter(for: session)
            }
        } catch {
            agents = []
            listedAgents = [:]
            chairRoute = nil
            agentsFailure = Self.message(for: error)
            return
        }

        do {
            var readings: [SessionReading] = []
            for session in sessions {
                async let agents = bus.agents(in: session)
                async let messages = bus.messages(in: session, after: 0)
                let values = try await (agents, messages)
                readings.append(SessionReading(
                    session: session, agents: values.0, messages: values.1
                ))
            }
            let digest = readings.flatMap {
                SwarmSessionAgents.digests(
                    sessionID: $0.session.id, agents: $0.agents, messages: $0.messages
                )
            }
            guard !Task.isCancelled else { return }
            if agents != digest { agents = digest }
            listedAgents = Dictionary(uniqueKeysWithValues: readings.flatMap { reading in
                reading.agents.map {
                    (InputRoute(sessionID: reading.session.id, agentID: $0.id), $0)
                }
            })
            chairRoute = readings.compactMap { reading -> InputRoute? in
                let chair = SwarmAgentID("orchestrator")
                guard reading.agents.contains(where: { $0.id == chair && $0.pane != nil }) else {
                    return nil
                }
                return InputRoute(sessionID: reading.session.id, agentID: chair)
            }.first
            agentsFailure = nil
        } catch is CancellationError {
            return
        } catch {
            agentsFailure = Self.message(for: error)
        }
    }

    func disabledReason(
        for agent: SwarmAgentID, in sessionID: SwarmSessionID?,
        target: SwarmSessionInputTarget
    ) -> String? {
        let route = route(for: agent, in: sessionID, target: target)
        let session = route.flatMap { route in sessions.first { $0.id == route.sessionID } }
        return SwarmSessionInteraction.disabledReason(
            adapter: session?.adapter ?? sessions.first?.adapter,
            pane: route.flatMap { listedAgents[$0]?.pane }, target: target
        )
    }

    func canSubmit(
        _ text: String, to agent: SwarmAgentID, in sessionID: SwarmSessionID?,
        target: SwarmSessionInputTarget
    ) -> Bool {
        disabledReason(for: agent, in: sessionID, target: target) == nil
            && text.contains { !$0.isWhitespace }
    }

    func inputFailure(for agent: SwarmAgentID, in sessionID: SwarmSessionID?) -> String? {
        route(for: agent, in: sessionID, target: sessionID == nil ? .chair : .agent)
            .flatMap { inputFailures[$0] }
    }

    func type(_ text: String, to agent: SwarmAgentID, in sessionID: SwarmSessionID?) async -> Bool {
        guard let route = route(
            for: agent, in: sessionID, target: sessionID == nil ? .chair : .agent
        ), let session = sessions.first(where: { $0.id == route.sessionID }) else { return false }
        do {
            try await bus.type(text, to: agent, in: session)
            inputFailures[route] = nil
            return true
        } catch {
            inputFailures[route] = Self.message(for: error)
            return false
        }
    }

    func interrupt(_ agent: SwarmAgentID, in sessionID: SwarmSessionID?) async {
        guard let route = route(
            for: agent, in: sessionID, target: sessionID == nil ? .chair : .agent
        ), let session = sessions.first(where: { $0.id == route.sessionID }) else { return }
        do {
            try await bus.interrupt(agent, in: session)
            inputFailures[route] = nil
        } catch {
            inputFailures[route] = Self.message(for: error)
        }
    }

    private func route(
        for agent: SwarmAgentID, in sessionID: SwarmSessionID?,
        target: SwarmSessionInputTarget
    ) -> InputRoute? {
        if target == .chair { return chairRoute }
        guard let sessionID else { return nil }
        return InputRoute(sessionID: sessionID, agentID: agent)
    }

    nonisolated private static func readChat(_ reader: TranscriptLogReader?) async -> ChairReading {
        let result = await ChairTranscriptOutput.read(reader)
        return await Task.detached(priority: .utility) {
            switch result {
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
        }.value
    }

    nonisolated private static func message(for error: any Error) -> String {
        switch error {
        case SwarmProfileError.unavailable(let text), SwarmProfileError.failed(let text): text
        default: error.localizedDescription
        }
    }
}

private struct InputRoute: Sendable, Hashable {
    var sessionID: SwarmSessionID
    var agentID: SwarmAgentID
}

private struct SessionReading: Sendable {
    var session: SwarmSession
    var agents: [SwarmAgent]
    var messages: [SwarmMessage]
}

private struct ChairReading: Sendable {
    var rows: [TranscriptRow] = []
    var droppedRows = 0
    var failure: String?
}
