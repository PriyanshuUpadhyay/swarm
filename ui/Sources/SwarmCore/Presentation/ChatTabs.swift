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
    /// Set for a chat the app is starting (ADR 0035). Such a tab offers no close or archive.
    public var pending: Pending? = nil

    public enum Pending: Sendable, Hashable { case starting, failed, closing }

    /// The chats the workspace is starting, newest first, then its chats. The tree lists the newest
    /// chat first, so a started chat keeps its place when its row replaces the pending tab. A row
    /// that a pending chat stands for is left out, so one chat never shows as two tabs.
    public static func tabs(
        _ chats: [ChatRow], pending: [PendingChat] = [], closing: Set<SwarmSessionID>, now: Int,
        chatNames: [String: String] = [:]
    ) -> [ChatTab] {
        let starting = Set(pending.compactMap(\.session))
        return pending.reversed().map { chat in
            let pending: Pending = switch chat.state {
            case .starting, .launched: .starting
            case .failed: .failed
            case .closing: .closing
            }
            return ChatTab(
                id: chat.tabID, title: "New chat", status: nil, badge: nil, canClose: false,
                pending: pending
            )
        } + chats.filter { !starting.contains($0.id) }.map { chat in
            ChatTab(
                id: chat.id.rawValue,
                title: ChatTitle.title(chat.session, appName: chatNames[ChatTitle.key(chat.session)]),
                status: chat.session.status,
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
