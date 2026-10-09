import Foundation

public struct GuardField: Identifiable, Sendable, Equatable {
    public let id = UUID()
    public var text: String

    public init(text: String) { self.text = text }
}

public struct GuardRuleFields: Identifiable, Sendable, Equatable {
    public let id = UUID()
    public var name: String
    public var event: String
    public var allTools: Bool
    public var tools: [GuardField]
    public var command: [GuardField]
    public var timeout: String

    public init(rule: GuardRules.Rule) {
        name = rule.name
        event = rule.event
        allTools = rule.tools == nil
        tools = (rule.tools ?? []).map { GuardField(text: $0) }
        command = rule.command.map { GuardField(text: $0) }
        timeout = rule.timeout.map(String.init) ?? ""
    }

    public var error: String? {
        do { try GuardRules(rules: [rule()]).validate(); return nil }
        catch { return error.localizedDescription }
    }

    public func rule() throws -> GuardRules.Rule {
        let seconds: UInt64?
        if timeout.isEmpty { seconds = nil }
        else if let parsed = UInt64(timeout) { seconds = parsed }
        else { throw GuardListError(reason: "Rule '\(name)' needs a whole timeout in seconds, or an empty field") }
        guard let executable = command.first, !executable.text.isEmpty else {
            throw GuardListError(reason: "Rule '\(name)' needs a command")
        }
        return .init(name: name, event: event, tools: allTools ? nil : tools.map(\.text),
                     command: command.map(\.text), timeout: seconds)
    }
}

public struct GuardRulesEditor: Sendable, Equatable {
    public var drafts: [GuardRuleFields] = []
    public private(set) var loadError: GuardListError?
    private var saveError: String?

    public init() {}

    public var error: String? {
        if let loadError, !loadError.isMissing {
            return "guards.json could not be read: \(loadError.reason). Every tool call is blocked until it is fixed."
        }
        if let saveError { return saveError }
        if loadError?.isMissing == true {
            return "guards.json is missing. Every tool call is blocked until it exists; press Save to create it."
        }
        return nil
    }

    public var canSave: Bool { (try? list()) != nil }
    public var canEdit: Bool { loadError == nil || loadError?.isMissing == true }

    public mutating func load(_ result: Result<GuardRules, GuardListError>) {
        saveError = nil
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
        guard canEdit else { return }
        drafts.append(GuardRuleFields(rule: .init(name: "New guard", command: [""])))
    }

    public mutating func delete(id: UUID) {
        guard canEdit else { return }
        drafts.removeAll { $0.id == id }
    }

    public func list() throws -> GuardRules {
        if let loadError, !loadError.isMissing { throw loadError }
        let list = GuardRules(rules: try drafts.map { try $0.rule() })
        try list.validate()
        return list
    }

    public mutating func recordSaveFailure(_ error: Error) {
        saveError = error.localizedDescription
    }

    public mutating func save(to url: URL) throws {
        do {
            try list().save(to: url)
            let result = GuardRules.load(url: url)
            load(result)
            _ = try result.get()
        } catch {
            recordSaveFailure(error)
            throw error
        }
    }
}
