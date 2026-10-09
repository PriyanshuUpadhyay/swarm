import Foundation

/// Concurrent state changes share one authorization request, including a denied or failed one.
public actor NoticeDelivery {
    private let authorize: @Sendable () async throws -> Bool
    private let deliver: @Sendable (Notice) async throws -> Void
    private var authorization: Task<Bool, Error>?

    public init(authorize: @escaping @Sendable () async throws -> Bool,
                deliver: @escaping @Sendable (Notice) async throws -> Void) {
        self.authorize = authorize
        self.deliver = deliver
    }

    public func post(_ notice: Notice) async throws {
        if authorization == nil {
            authorization = Task { [authorize] in try await authorize() }
        }
        guard let authorization, try await authorization.value else { return }
        try await deliver(notice)
    }
}

public enum NoticeDestination: Sendable, Hashable {
    case mainChat(SwarmSessionID), chatWindow(SwarmSessionID)

    public static func resolve(sessionID: SwarmSessionID, tree: SessionsTree,
                               openChats: Set<SwarmSessionID>) -> Self? {
        guard let chat = tree.session(sessionID) else { return nil }
        let windowID = ChatWindowState.id(for: chat)
        return openChats.contains(windowID) ? .chatWindow(windowID) : .mainChat(chat.id)
    }
}
