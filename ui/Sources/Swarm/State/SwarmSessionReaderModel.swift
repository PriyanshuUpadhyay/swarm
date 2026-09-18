import Foundation
import Observation
import SwarmCore

/// The live, read-only state for one discovered swarm session.
@MainActor
@Observable
final class SwarmSessionReaderModel {
    private let sessions: [SwarmSession]
    @ObservationIgnored private let bus: any SwarmBus

    private(set) var agents: [SwarmSessionAgentDigest] = []
    private(set) var agentsFailure: String?
    private var inputFailures: [InputRoute: String] = [:]
    private var listedAgents: [InputRoute: SwarmAgent] = [:]
    private var chairRoute: InputRoute?

    init(item: SwarmProjectSession, bus: any SwarmBus) {
        self.sessions = item.sessions
        self.bus = bus
        if item.sessions.allSatisfy({ (try? SwarmSessionInteraction.adapter(for: $0)) == nil }) {
            agentsFailure = SwarmSessionInteraction.missingAdapterSentence
        }
    }

    /// A reader with its answers already in it, for `SwarmSessionGallery`.
    ///
    /// The gallery is drawn by `ImageRenderer`, which runs nothing asynchronous, so a reader that
    /// can only be filled by awaiting the bus draws an empty column and photographs nothing worth
    /// looking at. Nothing else may use this: `refresh` is what fills `agents` in the app.
    init(showing agents: [SwarmSessionAgentDigest], in sessions: [SwarmSession]) {
        self.sessions = sessions
        self.bus = UnavailableSwarmBus()
        self.agents = agents
    }

    /// Refreshes when `~/.swarm` changes, and on `backstopSeconds` whether it changed or not.
    ///
    /// The once-a-second loop this replaces was the app's most expensive habit. `PerfLog` for
    /// 2026-09-18 recorded 2,089 passes over 100ms, median 164ms, for four sessions, and almost
    /// every one of them found exactly what the pass before had found. A bus that nobody is
    /// writing to now costs one sleeping task.
    ///
    /// The backstop is not belt and braces. `SwarmAgent.alive` is the adapter's pane list, and a
    /// pane that ends writes nothing to `~/.swarm`, so no file event can carry it. It is also the
    /// answer when `FSEventStreamStart` fails, which `WorktreeWatcher` reports by watching
    /// nothing.
    func follow() async {
        // One stream with two sources, so the loop below stays a plain `for await`.
        // `bufferingNewest(1)` is what makes a write storm one refresh: an agent writing a long
        // body produces many events, and every one of them asks the same question.
        let changes = AsyncStream<Void>(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let watcher = WorktreeWatcher { _ in continuation.yield() }
            watcher.watch(roots: [SwarmBusStore.shared.root])
            let backstop = Task {
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(Self.backstopSeconds))
                    continuation.yield()
                }
            }
            continuation.onTermination = { _ in
                watcher.stop()
                backstop.cancel()
            }
        }
        await refresh()
        for await _ in changes {
            if Task.isCancelled { return }
            await refresh()
        }
    }

    /// Five seconds, matching `SwarmPaneLiveness.interval`, because the liveness read is the one
    /// thing this tick exists to drive when no file has changed.
    static let backstopSeconds: Double = 5

    /// **Every observed property here is written only when it changes, and that is load-bearing.**
    ///
    /// `@Observable` fires on the assignment, not on a difference, so `agentsFailure = nil` once a
    /// second rebuilt the whole agents panel once a second whether or not anything had happened,
    /// and `listedAgents` did the same to every message field in it. The pane flickered while a
    /// chat ran because of that, not because the chat had new rows. `agents` was already guarded
    /// for this reason; the rest were not.
    private func refresh() async {
        do {
            for session in sessions {
                _ = try SwarmSessionInteraction.adapter(for: session)
            }
        } catch {
            if !agents.isEmpty { agents = [] }
            if !listedAgents.isEmpty { listedAgents = [:] }
            if chairRoute != nil { chairRoute = nil }
            let sentence = Self.message(for: error)
            if agentsFailure != sentence { agentsFailure = sentence }
            return
        }

        do {
            let busReadStarted = ContinuousClock.now
            defer {
                let duration = busReadStarted.duration(to: .now).components
                let milliseconds = Double(duration.seconds) * 1_000
                    + Double(duration.attoseconds) / 1e15
                if milliseconds > 100 {
                    PerfLog.shared.record(.busRead(
                        milliseconds: milliseconds, sessionCount: sessions.count
                    ))
                }
            }
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
            let listed = Dictionary(uniqueKeysWithValues: readings.flatMap { reading in
                reading.agents.map {
                    (InputRoute(sessionID: reading.session.id, agentID: $0.id), $0)
                }
            })
            if listedAgents != listed { listedAgents = listed }
            let chair = readings.compactMap { reading -> InputRoute? in
                let chair = SwarmAgentID("orchestrator")
                guard reading.agents.contains(where: { $0.id == chair && $0.pane != nil }) else {
                    return nil
                }
                return InputRoute(sessionID: reading.session.id, agentID: chair)
            }.first
            if chairRoute != chair { chairRoute = chair }
            if agentsFailure != nil { agentsFailure = nil }
        } catch is CancellationError {
            return
        } catch {
            let sentence = Self.message(for: error)
            if agentsFailure != sentence { agentsFailure = sentence }
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
