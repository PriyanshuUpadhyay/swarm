import Foundation

public enum SwarmChatHandoff {
    static let request = "Write a compact handoff summary for the next agent. Include the goal, decisions, work done, open tasks, and essential file paths. Do not start new work."

    /// Ask the old chair for a summary, start the new chair, then link the sessions after delivery.
    /// If the new chair fails after launch, its session stays separate so the old chat is intact.
    /// Cancellation is offered only before starting; it does not interrupt the old chair
    /// or undo a launched agent. Callers must keep later stages visible until they finish.
    public static func start(
        _ plan: SwarmChatLaunchPlan, after row: SwarmProjectSession,
        bus: any SwarmBus,
        onProgress: @Sendable (ChatSwitchPhase) async -> Void = { _ in }
    ) async throws -> SwarmSessionID {
        let timing = SwarmPerformance.begin("ModelHandoff")
        defer { timing.end() }
        guard plan.provider == "claude" || plan.provider == "codex" else {
            throw SwarmProfileError.failed("Model switching supports Claude and Codex chats")
        }
        try Task.checkCancellation()
        await onProgress(.preparing)
        let source = row.session
        let transcript = SwarmChairTranscript()
        let initial = await transcript.poll(session: source, chairProvider: row.provider)
        try Task.checkCancellation()
        guard case .rows(let oldRows, _) = initial else {
            throw SwarmProfileError.failed("The current chat has no readable transcript to carry forward")
        }

        var context: String?
        var isSummary = false
        let chair = try await bus.agents(in: source).first { $0.id == SwarmPanePolicy.chair }
        try Task.checkCancellation()
        if chair?.alive == true {
            guard !ChairTurn.isActive(oldRows) else {
                throw SwarmProfileError.failed("Wait for the current reply to finish, or stop it before switching model.")
            }
            await onProgress(.summarizing)
            try Task.checkCancellation()
            try await bus.type(request, to: SwarmPanePolicy.chair, in: source)
            for _ in 0..<90 {
                try await Task.sleep(for: .seconds(1))
                if case .rows(let rows, _) = await transcript.poll(session: source, chairProvider: row.provider),
                   let summary = completedSummary(in: rows, after: oldRows.count) {
                    context = summary
                    isSummary = true
                    break
                }
            }
        }
        if context == nil { context = recentContext(in: oldRows) }
        guard let context else {
            throw SwarmProfileError.failed("The current chat has no messages to carry forward")
        }

        try Task.checkCancellation()
        await onProgress(.starting)
        try Task.checkCancellation()
        let id = try await SwarmChatLauncher.start(plan, bus: bus)
        await onProgress(.waiting)
        for _ in 0..<30 {
            try await Task.sleep(for: .seconds(1))
            if let session = try await bus.sessions().first(where: { $0.id == id }),
               isReady(session, provider: plan.provider) {
                await onProgress(.delivering)
                try await bus.type(
                    firstMessage(context: context, isSummary: isSummary),
                    to: SwarmPanePolicy.chair, in: session
                )
                try await bus.linkChat(id, after: source.id)
                return id
            }
        }
        throw SwarmProfileError.failed("The new agent did not become ready to receive the summary")
    }

    /// SessionStart records the chair id before the first user turn creates a transcript log.
    static func isReady(_ session: SwarmSession, provider: String) -> Bool {
        session.chairProvider == provider && session.chairID != nil
    }

    static func completedSummary(in rows: [TranscriptRow], after baseline: Int) -> String? {
        guard rows.count > baseline,
              let question = rows[baseline...].firstIndex(where: {
                  $0.kind == .user && $0.text.contains(request)
              }),
              rows[question...].contains(where: \.endsTurn),
              let answer = rows[question...].last(where: { $0.kind == .assistant })?.text
                .trimmingCharacters(in: .whitespacesAndNewlines),
              !answer.isEmpty else { return nil }
        return String(answer.prefix(12_000))
    }

    static func recentContext(in rows: [TranscriptRow]) -> String? {
        let messages = rows.filter { $0.kind == .user || $0.kind == .assistant }
            .suffix(16)
            .map { row in
                let speaker = row.kind == .user ? "User" : "Agent"
                return "\(speaker): \(row.text.trimmingCharacters(in: .whitespacesAndNewlines))"
            }
        guard !messages.isEmpty else { return nil }
        return String(messages.joined(separator: "\n\n").suffix(12_000))
    }

    private static func firstMessage(context: String, isSummary: Bool) -> String {
        let heading = isSummary
            ? "The prior agent prepared this compact handoff summary:"
            : "These are recent messages from the prior chat:"
        let instruction = isSummary
            ? "Use this context for the next request. Confirm that you are ready, then wait."
            : "Make a compact working summary from this context. Confirm that you are ready, then wait."
        return "\(heading)\n\n\(context)\n\n\(instruction)"
    }
}
