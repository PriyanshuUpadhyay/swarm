import Foundation

/// Concurrent posts share an in-flight request; each later post reads permission again.
public actor NoticeDelivery {
    private struct Authorization {
        let id = UUID()
        let task: Task<Bool, Error>
    }

    private let authorize: @Sendable () async throws -> Bool
    private let deliver: @Sendable (Notice) async throws -> Void
    private let onDenied: @Sendable () async -> Void
    private var authorization: Authorization?
    private var reportedDenial = false

    public init(authorize: @escaping @Sendable () async throws -> Bool,
                deliver: @escaping @Sendable (Notice) async throws -> Void,
                onDenied: @escaping @Sendable () async -> Void) {
        self.authorize = authorize
        self.deliver = deliver
        self.onDenied = onDenied
    }

    public func post(_ notice: Notice) async throws {
        let request: Authorization
        if let cached = authorization { request = cached }
        else {
            request = Authorization(task: Task { [authorize] in try await authorize() })
            authorization = request
        }
        let granted: Bool
        do { granted = try await request.task.value }
        catch {
            if authorization?.id == request.id { authorization = nil }
            throw error
        }
        // An older waiter must not clear a newer request started during actor suspension.
        if authorization?.id == request.id { authorization = nil }
        guard granted else {
            if !reportedDenial {
                reportedDenial = true
                await onDenied()
            }
            return
        }
        try await deliver(notice)
    }
}

/// A notice click has only a host focus action. A failure posts one notice with no click action.
public enum NoticeClick {
    public static func receive(session: SwarmSessionID?, agent: SwarmAgentID?,
                               focus: @Sendable (SwarmAgentID, SwarmSessionID) async throws -> Void,
                               postError: @Sendable (Notice) async -> Void) async {
        do {
            guard let session else { throw SwarmProfileError.failed("swarm: pane closed") }
            try await focus(agent ?? SwarmAgentID("orchestrator"), session)
        } catch {
            await postError(Notice(title: "Swarm", body: error.localizedDescription))
        }
    }
}
