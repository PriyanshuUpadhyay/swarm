public struct ChatBanner: Sendable, Equatable {
    public enum Source: Sendable, Hashable { case action, refresh, model }

    public struct Message: Sendable, Equatable {
        public let source: Source
        public let text: String
        fileprivate let revision: Int
    }

    private var slots: [Source: Message] = [:]
    private var revision = 0
    private var modelText: String?
    private var refreshFailureToken = 0
    private var lastSuccessToken = 0
    public private(set) var announcement: String?

    public init() {}

    public var visible: Message? { slots[.action] ?? slots[.refresh] ?? slots[.model] }

    public mutating func setAction(_ text: String?) {
        set(text, in: .action)
    }

    public mutating func setModel(_ text: String?) {
        announcement = nil
        // Dismiss keeps an unchanged model error hidden until the model's text changes.
        guard text != modelText else { return }
        modelText = text
        set(text, in: .model)
    }

    public mutating func refreshFailed(_ text: String, token: Int) {
        announcement = nil
        guard token >= lastSuccessToken else { return }
        refreshFailureToken = token
        set(text, in: .refresh)
    }

    public mutating func refreshSucceeded(token: Int) {
        announcement = nil
        guard token >= lastSuccessToken else { return }
        lastSuccessToken = token
        if refreshFailureToken <= token { set(nil, in: .refresh) }
    }

    public mutating func dismiss() {
        announcement = nil
        if let visible { set(nil, in: visible.source) }
    }

    private mutating func set(_ text: String?, in source: Source) {
        let previous = visible
        if let text {
            revision += 1
            slots[source] = Message(source: source, text: text, revision: revision)
        } else {
            slots[source] = nil
        }
        // A hidden update stays silent; each newly visible failure is spoken, including a reveal.
        announcement = visible != previous ? visible?.text : nil
    }
}
