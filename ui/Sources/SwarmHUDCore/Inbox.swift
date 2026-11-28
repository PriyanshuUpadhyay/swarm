import Foundation

// ADR 0147: the HUD is the notice inbox. `swarm notices --inbox --json` gives the agents that wait
// on the owner, with the prompt on their screen, and the newest unseen result of each chat.

public struct InboxSnapshot: Decodable, Equatable, Sendable {
    public var needs: [NeedCard]
    public var rows: [InboxRow]
    /// The full counts; the store cuts each list, so "+N more" and the wing read these.
    public var needsTotal: Int
    public var rowsTotal: Int
    /// ADR 0156: the chats with a working or waiting agent, and their full count. An older swarm
    /// sends none.
    public var active: [ActiveChat]
    public var activeTotal: Int
    /// Settings' "Play a sound"; on when an older swarm does not say.
    public var sound: Bool

    public init(needs: [NeedCard] = [], rows: [InboxRow] = [], rowsTotal: Int? = nil, active: [ActiveChat] = [],
                sound: Bool = true) {
        (self.needs, self.rows, self.active, self.sound) = (needs, rows, active, sound)
        (self.needsTotal, self.rowsTotal, self.activeTotal) = (needs.count, rowsTotal ?? rows.count, active.count)
    }

    private enum CodingKeys: String, CodingKey {
        case schema, needs, rows, active, sound
        case needsTotal = "needs_total"
        case rowsTotal = "rows_total"
        case activeTotal = "active_total"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let schema = try container.decode(String.self, forKey: .schema)
        guard schema == "swarm.inbox.v1" else {
            throw DecodingError.dataCorruptedError(forKey: .schema, in: container, debugDescription: "unknown schema \(schema)")
        }
        // A bad entry costs only that entry.
        needs = try container.decodeIfPresent([Lossy<NeedCard>].self, forKey: .needs)?.compactMap(\.value) ?? []
        rows = try container.decodeIfPresent([Lossy<InboxRow>].self, forKey: .rows)?.compactMap(\.value) ?? []
        needsTotal = try container.decode(Int.self, forKey: .needsTotal)
        rowsTotal = try container.decode(Int.self, forKey: .rowsTotal)
        active = try container.decodeIfPresent([Lossy<ActiveChat>].self, forKey: .active)?.compactMap(\.value) ?? []
        activeTotal = try container.decodeIfPresent(Int.self, forKey: .activeTotal) ?? active.count
        sound = try container.decodeIfPresent(Bool.self, forKey: .sound) ?? true
    }

    /// Nil for another schema or a reply that does not decode, so the HUD keeps its last inbox
    /// and does not pop every row again when the next good read comes.
    public static func read(_ data: Data) -> InboxSnapshot? {
        try? JSONDecoder().decode(InboxSnapshot.self, from: data)
    }
}

/// What an inbox line says, with the words that the store writes (`notice_kind` in store.rs).
public enum InboxKind: String, Decodable, Sendable {
    case done, asks, failed
    /// A live waiting agent; the store's inbox rows never hold it.
    case waiting

    public var word: String {
        switch self {
        case .done: return "done"
        case .asks: return "asks"
        case .failed: return "stopped"
        case .waiting: return "needs you"
        }
    }

    /// A done line closes on its own; a question, a stop, or a wait waits for the owner.
    public var stays: Bool { self != .done }
}

/// A chat's newest unseen result: `done`, `asks` (its reply ends with a question), or `failed`.
public struct InboxRow: Decodable, Equatable, Sendable {
    public let id: Int64
    public let session: String
    public let agent: String?
    public let kind: InboxKind
    public let title: String
    public let body: String

    public init(id: Int64, session: String, agent: String? = nil, kind: InboxKind, title: String, body: String) {
        (self.id, self.session, self.agent, self.kind, self.title, self.body) = (id, session, agent, kind, title, body)
    }

    private enum CodingKeys: String, CodingKey {
        case id, kind, title, body
        case session = "session_id"
        case agent = "agent_id"
    }
}

/// ADR 0156: a chat that still works, so its last "done" is not its end.
public struct ActiveChat: Decodable, Equatable, Sendable {
    public let session: String
    public let title: String
    public let working: Int
    public let blocked: Int

    public init(session: String, title: String, working: Int, blocked: Int = 0) {
        (self.session, self.title, self.working, self.blocked) = (session, title, working, blocked)
    }
}

/// An agent that waits on the owner. `prompt` is nil when its screen could not be read; the card
/// then offers Open only.
public struct NeedCard: Decodable, Equatable, Sendable {
    public let session: String
    public let agent: String
    public let adapter: String?
    public let title: String
    public let prompt: NeedPrompt?

    public var key: String { "\(session)/\(agent)/\(prompt?.id ?? "")" }
}

/// The question on the agent's screen; `id` is the one `swarm answer` accepts, and a choice is
/// answered by its index.
public struct NeedPrompt: Decodable, Equatable, Sendable {
    public let id: String
    public let question: String
    public let choices: [String]
    /// Optional presentation kind; an unknown or missing kind uses the ordinary question card.
    public let kind: String?
}

/// One line of the popup under the notch.
public struct PopupItem: Equatable, Sendable {
    public let key: String
    public let session: String
    public let agent: String?
    public let title: String
    public let kind: InboxKind
    public let body: String
    public let word: String

    public var stays: Bool { kind.stays }
    /// A line for an Open that failed; it is not in the inbox, so a read does not prune it.
    public var openFailed: Bool { key.hasPrefix("open-failed/") }

    public init(row: InboxRow) {
        key = "row/\(row.id)"
        (session, agent, kind, title, body) = (row.session, row.agent, row.kind, row.title, row.body)
        word = kind.word
    }

    /// The HUD closes before `swarm focus` answers, so a failed Open comes back as a line that
    /// stays; a click on it tries again.
    public init(openFailed session: String, agent: String?, title: String, error: String) {
        key = "open-failed/\(session)"
        (self.session, self.agent, self.title, kind, body) = (session, agent, title, .failed, error)
        word = "did not open"
    }

    public init(need: NeedCard) {
        // No prompt id, so a screen read that fails once does not pop the same wait again.
        key = "need/\(need.session)/\(need.agent)"
        (session, agent, title, kind) = (need.session, need.agent, need.title, .waiting)
        body = need.prompt?.question.split(separator: "\n").first.map(String.init) ?? "Waiting on a permission."
        word = kind.word
    }
}

public struct Popup: Equatable, Sendable {
    public var items: [PopupItem]
    public var startedAt: Date
    public var lastAt: Date

    public init(items: [PopupItem], startedAt: Date, lastAt: Date? = nil) {
        (self.items, self.startedAt, self.lastAt) = (items, startedAt, lastAt ?? startedAt)
    }

    /// A done-only popup closes this long after its last item came in.
    public static let peek: TimeInterval = 4
    /// Items that come in this long after the popup opened join it, so a burst is one popup.
    public static let merge: TimeInterval = 10
    /// The popup shows at most this many lines; the open card lists the rest.
    public static let maxItems = 3

    public var stays: Bool { items.contains(where: \.stays) }

    public func expired(now: Date, hovered: Bool) -> Bool {
        !hovered && !stays && now.timeIntervalSince(lastAt) >= Popup.peek
    }
}

/// The popup items for what is new in `inbox`: a need or row whose key `known` lacks. The
/// first read of a HUD passes `known: nil`, so a HUD that starts shows nothing old.
public func newPopupItems(_ inbox: InboxSnapshot, known: Set<String>?) -> [PopupItem] {
    guard let known else { return [] }
    let items = inbox.needs.map(PopupItem.init(need:)) + inbox.rows.map(PopupItem.init(row:))
    return items.filter { !known.contains($0.key) }
}

public func inboxKeys(_ inbox: InboxSnapshot) -> Set<String> {
    Set(inbox.needs.map { PopupItem(need: $0).key } + inbox.rows.map { PopupItem(row: $0).key })
}

/// `new` joins `popup` inside its merge window, else starts a new one, which keeps the old lines that
/// stay, such as a question. A chat keeps one line: its newer item takes the place of the older.
public func mergePopup(_ popup: Popup?, new: [PopupItem], now: Date) -> Popup? {
    guard !new.isEmpty else { return popup }
    var result = popup.flatMap { now.timeIntervalSince($0.startedAt) < Popup.merge ? $0 : nil }
        ?? Popup(items: popup?.items.filter(\.stays) ?? [], startedAt: now)
    // `new` is newest first, as the inbox lists it, so the newest ends on top.
    for item in new.reversed() {
        result.items.removeAll { $0.session == item.session }
        result.items.insert(item, at: 0)
    }
    result.items = Array(result.items.prefix(Popup.maxItems))
    result.lastAt = now
    return result
}

/// The popup without the items of chats the owner has seen, for example from the app; nil when
/// none is left.
public func prunePopup(_ popup: Popup?, inbox: InboxSnapshot) -> Popup? {
    guard var popup else { return nil }
    let live = inboxKeys(inbox)
    popup.items.removeAll { !$0.openFailed && !live.contains($0.key) }
    return popup.items.isEmpty ? nil : popup
}

/// The new items that get a popup. Swarm.app in front shows a done, asks, or stopped chat itself,
/// so only a wait pops then: the blocked agent can be in a chat that the owner does not look at.
public func popupItems(_ new: [PopupItem], swarmInFront: Bool) -> [PopupItem] {
    swarmInFront ? new.filter { $0.kind == .waiting } : new
}

/// ADR 0156: a new done or asks row means a chat converged: its chair ended its turn while no child
/// worked. A new wait plays the sound too, because the agent is blocked until the owner acts.
/// `new` is empty on a HUD's first read, so a HUD that starts plays nothing.
public func playsSound(_ inbox: InboxSnapshot, new: [PopupItem]) -> Bool {
    inbox.sound && new.contains { $0.kind != .failed }
}

/// The closed wing's words for the inbox, or nil to leave the working count. A need wins, then a
/// stopped chat, then done chats.
public func inboxWing(_ inbox: InboxSnapshot) -> WingText? {
    if let need = inbox.needs.first {
        return WingText(left: InboxKind.waiting.word, blocked: true, tags: [String(need.title.prefix(16))])
    }
    if inbox.rows.contains(where: { $0.kind == .failed }) {
        return WingText(left: InboxKind.failed.word, dot: .failed)
    }
    if !inbox.rows.isEmpty {
        return WingText(left: "\(inbox.rowsTotal) \(InboxKind.done.word)", dot: .done)
    }
    return nil
}
