import Foundation

public struct PaneWidths: Sendable, Hashable {
    public var column: Double?
    public var chat: Double?
    public var splits: String

    public init(column: Double? = nil, chat: Double? = nil, splits: String = "") {
        self.column = column
        self.chat = chat
        self.splits = splits
    }
}

public struct ChatWindowState: Sendable, Hashable {
    public var selection: SwarmSessionID
    public var paneWidths: PaneWidths

    public init(selection: SwarmSessionID, paneWidths: PaneWidths = .init()) {
        self.selection = selection
        self.paneWidths = paneWidths
    }

    /// A model switch keeps the first session's chat key, so it also keeps its window identity.
    public static func id(for chat: SwarmProjectSession) -> SwarmSessionID {
        SwarmSessionID(ChatTitle.key(chat))
    }
}
