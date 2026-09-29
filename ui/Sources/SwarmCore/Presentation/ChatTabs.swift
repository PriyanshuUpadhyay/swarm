import Foundation

/// One chat tab, as plain values.
public struct ChatTab: Sendable, Hashable, Identifiable {
    public let id: String
    public let title: String
    public let status: AgentStatus?
    /// One letter for the chat's provider, such as C for Claude or X for Codex.
    public let badge: String?
    /// A live chat can be closed; an ended one or one already closing cannot.
    public let canClose: Bool

    public static func tabs(_ chats: [ChatRow], closing: Set<SwarmSessionID>, now: Int) -> [ChatTab] {
        chats.map { chat in
            ChatTab(
                id: chat.id.rawValue, title: chat.session.title, status: chat.session.status,
                badge: chat.session.provider.map(badge),
                canClose: SessionRowPresentation.make(chat, now: now).state == .live
                    && !closing.contains(chat.id)
            )
        }
    }

    public static func badge(_ provider: String) -> String {
        switch provider.lowercased() {
        case "codex": "X"
        case "agy": "A"
        default: provider.prefix(1).uppercased()
        }
    }
}
