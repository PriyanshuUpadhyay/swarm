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
    public var pr: String?
    public var ci: String?
    public init(dirtyCount: Int? = nil, pr: String? = nil, ci: String? = nil) {
        self.dirtyCount = dirtyCount
        self.pr = pr
        self.ci = ci
    }
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
                SwarmPanePolicy.isChair($0, in: session)
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
                !SwarmPanePolicy.isChair($0, in: session)
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
            context.text[field] = unique(contexts.compactMap { $0.text[field] })
        }
        context.text[.branch] = entries.compactMap(\.workspace.branch).joined(separator: " · ")
        context.text[.children] = chats.count == 1 ? "1 chat" : "\(chats.count) chats"
        if let latest = chats.max(by: { $0.lastActivity < $1.lastActivity }) {
            context.text[.age] = chatContext(latest, title: "", navigation: navigation, now: now, agentsBySession: agentsBySession).text[.age]
        }
        let chairs = chats.flatMap { chat in
            chat.sessions.compactMap { session in
                (agentsBySession[session.id] ?? []).first {
                    SwarmPanePolicy.isChair($0, in: session)
                }
            }
        }
        setUsage(chairs, in: &context)
        let dirty = entries.compactMap { workspaceFields[$0.id]?.dirtyCount }
        if !dirty.isEmpty { context.text[.dirty] = "\(dirty.reduce(0, +)) dirty" }
        context.text[.steps] = SidebarRows.runSummary(entries.flatMap { runsByWorkspace[$0.id] ?? [] })
        let cached = entries.compactMap { workspaceFields[$0.id] }
        context.text[.pr] = unique(cached.compactMap(\.pr))
        context.text[.ci] = unique(cached.compactMap(\.ci))
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
        if !costs.isEmpty { context.text[.cost] = costs.reduce(0, +).formatted(.currency(code: "USD")) }
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
        context.text[.pr] = workspace.pr
        context.text[.ci] = workspace.ci
    }

    private static func unique(_ texts: [String]) -> String {
        var seen: Set<String> = []
        return texts.filter { seen.insert($0).inserted }.joined(separator: " · ")
    }

    public static func githubFields(_ lookup: PullRequestLookup) -> RowWorkspaceFields {
        guard let request = lookup.match?.pullRequest else { return RowWorkspaceFields() }
        return RowWorkspaceFields(pr: "PR #\(request.number)", ci: ciText(request.statusCheckRollup ?? []))
    }

    private static func ciText(_ checks: [GitHubCheck]) -> String? {
        guard !checks.isEmpty else { return nil }
        let states = checks.map { $0.result.uppercased() }
        let failed: Set<String> = ["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE", "STALE"]
        let pending: Set<String> = ["PENDING", "EXPECTED", "QUEUED", "IN_PROGRESS", "WAITING", "REQUESTED"]
        let passed: Set<String> = ["SUCCESS", "NEUTRAL", "SKIPPED"]
        if states.contains(where: failed.contains) { return "CI failed" }
        if states.contains(where: pending.contains) { return "CI pending" }
        return states.allSatisfy(passed.contains) ? "CI passed" : "CI unknown"
    }

    public static func stepsByChat(
        in entry: WorkspaceEntry, runs: [StepRun], agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> [SwarmSessionID: String] {
        var result: [SwarmSessionID: String] = [:]
        for run in SidebarRows.orderedRuns(runs) {
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
    public static let refreshInterval: TimeInterval = 10
    public static let githubInterval: TimeInterval = 120
    public typealias Inspect = @Sendable (String) async throws -> GitWorkspaceSnapshot
    public typealias Lookup = @Sendable (GitWorkspaceSnapshot) async throws -> PullRequestLookup
    private let inspect: Inspect
    private let lookup: Lookup
    private var lastDirtyRead: [String: Date] = [:]
    private var lastGitHubRead: [String: Date] = [:]
    private var values: [String: RowWorkspaceFields] = [:]

    public init(
        inspect: @escaping Inspect = { try await Git.inspect(in: $0) },
        lookup: @escaping Lookup = { try await GitHubInspection().lookup(in: $0) }
    ) {
        self.inspect = inspect
        self.lookup = lookup
    }

    public func refresh(paths: [String], githubPaths: [String] = [], now: Date? = nil) async -> [String: RowWorkspaceFields] {
        let dirty = Set(paths)
        let github = Set(githubPaths)
        for path in dirty.union(github).sorted() {
            guard !Task.isCancelled else { break }
            let readTime = now ?? Date()
            let readDirty = dirty.contains(path) && due(lastDirtyRead[path], after: Self.refreshInterval, now: readTime)
            let readGitHub = github.contains(path) && due(lastGitHubRead[path], after: Self.githubInterval, now: readTime)
            guard readDirty || readGitHub else { continue }
            if readDirty { lastDirtyRead[path] = readTime }
            if readGitHub { lastGitHubRead[path] = readTime }
            do {
                let snapshot = try await inspect(path)
                if readDirty {
                    values[path, default: RowWorkspaceFields()].dirtyCount = Set(snapshot.files.map(\.path)).count
                }
                if readGitHub {
                    lastGitHubRead[path] = now ?? Date()
                    do {
                        let fields = RowFields.githubFields(try await lookup(snapshot))
                        values[path, default: RowWorkspaceFields()].pr = fields.pr
                        values[path, default: RowWorkspaceFields()].ci = fields.ci
                    } catch {
                        clearGitHub(path)
                    }
                }
            } catch {
                if readDirty { values[path, default: RowWorkspaceFields()].dirtyCount = nil }
                if readGitHub { clearGitHub(path) }
            }
        }
        return values
    }

    private func due(_ last: Date?, after seconds: TimeInterval, now: Date) -> Bool {
        last.map { now.timeIntervalSince($0) >= seconds } ?? true
    }

    private func clearGitHub(_ path: String) {
        values[path, default: RowWorkspaceFields()].pr = nil
        values[path, default: RowWorkspaceFields()].ci = nil
    }
}
