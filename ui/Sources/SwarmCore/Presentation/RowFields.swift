import Foundation

public enum RowField: String, Codable, CaseIterable, Sendable {
    case status, title, provider, branch, age, children, question, unread, dirty, model, effort, steps, pr, ci, tokens, cost
}

/// The owner picks the order separately for each surface. Unknown names are skipped so a
/// choices file from a newer app still keeps the fields this app knows (ADR 0052).
public struct RowFieldLists: Codable, Equatable, Sendable {
    public var project: [RowField] = [.title, .status]
    public var workspace: [RowField] = [.status, .title, .branch, .children, .age, .steps]
    public var chat: [RowField] = [.status, .title, .age, .steps, .children, .unread, .tokens]
    public var tab: [RowField] = [.status, .title, .provider, .unread, .tokens]

    public init() {}

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        func fields(_ key: CodingKeys, default bundled: [RowField]) throws -> [RowField] {
            try values.decodeIfPresent([String].self, forKey: key)?.compactMap(RowField.init(rawValue:)) ?? bundled
        }
        project = try fields(.project, default: project)
        workspace = try fields(.workspace, default: workspace)
        chat = try fields(.chat, default: chat)
        tab = try fields(.tab, default: tab)
    }
}

public struct RowFieldValue: Sendable, Hashable {
    public let field: RowField
    public let text: String
    public var status: AgentStatus? = nil
}

public struct RowWorkspaceFields: Sendable, Equatable {
    public var dirtyCount: Int?
    public init(dirtyCount: Int? = nil) { self.dirtyCount = dirtyCount }
}

public struct RowFieldContext: Sendable {
    public var text: [RowField: String]
    public var status: AgentStatus?
    public var unread: Bool

    public init(title: String, status: AgentStatus? = nil, unread: Bool = false, text: [RowField: String] = [:]) {
        self.text = text
        self.text[.title] = title
        self.status = status
        self.unread = unread
    }

    public func values(_ fields: [RowField]) -> [RowFieldValue] {
        fields.compactMap { field in
            switch field {
            case .status:
                return status.map { RowFieldValue(field: field, text: $0.rawValue, status: $0) }
            case .unread:
                return unread ? RowFieldValue(field: field, text: "Unread") : nil
            default:
                return text[field].flatMap { $0.isEmpty ? nil : RowFieldValue(field: field, text: $0) }
            }
        }
    }
}

public enum RowFields {
    public static func chatContext(
        _ chat: SwarmProjectSession, title: String, navigation: WorkspaceNavigation, now: Int,
        agentsBySession: [SwarmSessionID: [SwarmAgent]], branch: String? = nil,
        workspace: RowWorkspaceFields = .init(), children: String? = nil, steps: String? = nil
    ) -> RowFieldContext {
        let chairs = chat.sessions.compactMap { session in
            (agentsBySession[session.id] ?? []).first {
                $0.id == SwarmPanePolicy.chair || $0.id.rawValue == session.chairID?.rawValue
            }
        }
        let agents = chat.sessions.flatMap { agentsBySession[$0.id] ?? [] }
        var context = RowFieldContext(title: title, status: chat.status, unread: navigation.isUnread(chat))
        context.text[.provider] = chat.provider ?? chairs.first?.provider
        context.text[.branch] = branch
        context.text[.age] = SessionRowPresentation.make(ChatRow(session: chat, workspace: "", workspacePath: chat.session.cwd), now: now).age
        context.text[.children] = children ?? childCount(chat, agentsBySession: agentsBySession)
        context.text[.question] = agents.first { $0.prompt != nil }?.prompt?.question
            ?? agents.first { $0.status == .waiting }?.stateDetail
        context.text[.model] = chairs.first?.model
        context.text[.effort] = chairs.first?.effort
        context.text[.steps] = steps
        setUsage(chairs, in: &context)
        setWorkspace(workspace, in: &context)
        return context
    }

    static func childCount(_ chat: SwarmProjectSession, agentsBySession: [SwarmSessionID: [SwarmAgent]]) -> String? {
        let count = chat.sessions.reduce(0) { total, session in
            total + (agentsBySession[session.id] ?? []).filter {
                $0.id != SwarmPanePolicy.chair && $0.id.rawValue != session.chairID?.rawValue
            }.count
        }
        return count == 0 ? nil : count == 1 ? "1 agent" : "\(count) agents"
    }

    public static func workspaceContext(
        _ entries: [WorkspaceEntry], title: String, navigation: WorkspaceNavigation, now: Int,
        agentsBySession: [SwarmSessionID: [SwarmAgent]], workspaceFields: [String: RowWorkspaceFields],
        runsByWorkspace: [String: [StepRun]] = [:]
    ) -> RowFieldContext {
        let chats = entries.flatMap(\.chats)
        let contexts = chats.map { chatContext($0, title: "", navigation: navigation, now: now, agentsBySession: agentsBySession) }
        var context = RowFieldContext(title: title, status: AgentStatus.aggregate(entries.compactMap(\.status)),
                                      unread: chats.contains { navigation.isUnread($0) })
        for field in [RowField.provider, .model, .effort, .question] {
            let texts = contexts.compactMap { $0.text[field] }
            var seen: Set<String> = []
            context.text[field] = texts.filter { seen.insert($0).inserted }.joined(separator: " · ")
        }
        context.text[.branch] = entries.compactMap(\.workspace.branch).joined(separator: " · ")
        context.text[.children] = chats.count == 1 ? "1 chat" : "\(chats.count) chats"
        if let latest = chats.max(by: { $0.lastActivity < $1.lastActivity }) {
            context.text[.age] = chatContext(latest, title: "", navigation: navigation, now: now, agentsBySession: agentsBySession).text[.age]
        }
        let chairs = chats.flatMap { chat in
            chat.sessions.compactMap { session in
                (agentsBySession[session.id] ?? []).first {
                    $0.id == SwarmPanePolicy.chair || $0.id.rawValue == session.chairID?.rawValue
                }
            }
        }
        setUsage(chairs, in: &context)
        let dirty = entries.compactMap { workspaceFields[$0.id]?.dirtyCount }
        if !dirty.isEmpty { context.text[.dirty] = "\(dirty.reduce(0, +)) dirty" }
        context.text[.steps] = SidebarRows.runSummary(entries.flatMap { runsByWorkspace[$0.id] ?? [] })
        return context
    }

    private static func setUsage(_ agents: [SwarmAgent], in context: inout RowFieldContext) {
        let tokens = agents.compactMap(\.tokens)
        if !tokens.isEmpty {
            let total = tokens.reduce(Int64(0)) { total, value in
                let sum = total.addingReportingOverflow(value)
                return sum.overflow ? Int64.max : sum.partialValue
            }
            context.text[.tokens] = "\(total) tokens"
        }
        let costs = agents.compactMap(\.costUsd)
        if !costs.isEmpty { context.text[.cost] = String(format: "$%.2f", costs.reduce(0, +)) }
    }

    public static func addWorkspace(_ workspace: RowWorkspaceFields, to context: inout RowFieldContext) {
        setWorkspace(workspace, in: &context)
    }

    public static func agentContext(_ agent: SwarmAgent, title: String) -> RowFieldContext {
        var context = RowFieldContext(title: title, status: agent.status)
        context.text[.provider] = agent.provider
        context.text[.model] = agent.model
        context.text[.effort] = agent.effort
        context.text[.question] = agent.prompt?.question ?? (agent.status == .waiting ? agent.stateDetail : nil)
        setUsage([agent], in: &context)
        return context
    }

    private static func setWorkspace(_ workspace: RowWorkspaceFields, in context: inout RowFieldContext) {
        if let count = workspace.dirtyCount { context.text[.dirty] = "\(count) dirty" }
    }

    public static func stepsByChat(
        in entry: WorkspaceEntry, runs: [StepRun], agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> [SwarmSessionID: String] {
        var result: [SwarmSessionID: String] = [:]
        let ordered = runs.filter { !$0.closed }.sorted {
            $0.urgency == $1.urgency ? $0.id < $1.id : $0.urgency > $1.urgency
        }
        for run in ordered {
            for step in run.steps {
                if let chat = SidebarRows.chat(for: step, in: entry, agentsBySession: agentsBySession), result[chat.id] == nil {
                    result[chat.id] = "\(run.skill) · \(step.title)"
                }
            }
        }
        return result
    }

    public static func requestedPaths(for field: RowField, entries: [WorkspaceEntry], fields: RowFieldLists) -> [String] {
        let allWorkspaces = fields.project.contains(field) || fields.workspace.contains(field)
        let chats = fields.chat.contains(field) || fields.tab.contains(field)
        return entries.filter {
            !$0.workspace.missing && !$0.workspace.isRemoved && (allWorkspaces || (chats && !$0.chats.isEmpty))
        }.map(\.id)
    }
}

/// Each attempted read reserves its interval before awaiting, so concurrent refreshes and
/// failed reads cannot cause a burst of subprocesses for the same workspace.
public actor RowFieldCache {
    public typealias Inspect = @Sendable (String) async throws -> GitWorkspaceSnapshot
    private let inspect: Inspect
    private var lastDirtyRead: [String: Date] = [:]
    private var values: [String: RowWorkspaceFields] = [:]

    public init(inspect: @escaping Inspect = { try await Git.inspect(in: $0) }) { self.inspect = inspect }

    public func refresh(paths: [String], now: Date = Date()) async -> [String: RowWorkspaceFields] {
        for path in Set(paths).sorted() {
            guard !Task.isCancelled else { break }
            if let last = lastDirtyRead[path], now.timeIntervalSince(last) < 10 { continue }
            lastDirtyRead[path] = now
            do {
                let snapshot = try await inspect(path)
                values[path, default: RowWorkspaceFields()].dirtyCount = Set(snapshot.files.map(\.path)).count
            } catch {
                values[path, default: RowWorkspaceFields()].dirtyCount = nil
            }
        }
        return values
    }
}
