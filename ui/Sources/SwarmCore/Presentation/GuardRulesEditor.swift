import Foundation

public struct GuardRuleFields: Sendable, Equatable {
    public var name: String
    public var event: String
    public var tools: [String]?
    public var command: [String]
    public var timeout: String

    public init(rule: GuardRules.Rule) {
        name = rule.name
        event = rule.event
        tools = rule.tools
        command = rule.command
        timeout = rule.timeout.map(String.init) ?? ""
    }

    public func rule() throws -> GuardRules.Rule {
        let seconds: UInt64?
        if timeout.isEmpty { seconds = nil }
        else if let parsed = UInt64(timeout) { seconds = parsed }
        else { throw GuardListError(reason: "Rule '\(name)' needs a whole timeout in seconds, or an empty field") }
        guard let executable = command.first, !executable.isEmpty else {
            throw GuardListError(reason: "Rule '\(name)' needs a command")
        }
        return .init(name: name, event: event, tools: tools, command: command, timeout: seconds)
    }
}

public struct GuardRulesEditor: Sendable, Equatable {
    public var drafts: [GuardRuleFields] = []
    public private(set) var loadError: GuardListError?

    public init() {}

    public var error: String? {
        if let loadError { return loadError.reason }
        do { _ = try list(); return nil }
        catch { return error.localizedDescription }
    }

    public var canSave: Bool { error == nil }

    public mutating func load(_ result: Result<GuardRules, GuardListError>) {
        switch result {
        case .success(let list):
            drafts = list.rules.map(GuardRuleFields.init)
            loadError = nil
        case .failure(let error):
            drafts = []
            loadError = error
        }
    }

    public mutating func add() {
        guard loadError == nil else { return }
        drafts.append(GuardRuleFields(rule: .init(name: "New guard", command: [""])))
    }

    public mutating func delete(at index: Int) {
        guard loadError == nil, drafts.indices.contains(index) else { return }
        drafts.remove(at: index)
    }

    public func list() throws -> GuardRules {
        if let loadError { throw loadError }
        let list = GuardRules(rules: try drafts.map { try $0.rule() })
        try list.validate()
        return list
    }

    public mutating func save(to url: URL) throws {
        try list().save(to: url)
        let result = GuardRules.load(url: url)
        load(result)
        _ = try result.get()
    }
}
