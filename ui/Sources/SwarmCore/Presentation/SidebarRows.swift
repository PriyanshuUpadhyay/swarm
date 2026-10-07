import Foundation

/// One row of the sidebar, as plain values.
public struct SidebarRow: Sendable, Hashable, Identifiable {
    public enum Kind: Sendable, Hashable { case workspace, chat, child, more }
    public let id: String
    public let kind: Kind
    public let depth: Int
    public let parentID: String?
    public let expanded: Bool
    public let hasChildren: Bool
    public let childrenSummary: String?
    public let title: String
    /// Branch, project, or a path qualifier; shown after the title in secondary text.
    public let detail: String
    public let status: AgentStatus?
    /// Agents per status, most urgent first; shown on hover.
    public let counts: [StatusCount]
    public let age: String?
    public let help: String
    public let pinned: Bool
    public let archived: Bool
    public let missing: Bool
    public let newChatEnabled: Bool
    public var run: SidebarRun? = nil
    public var runSummary: String? = nil
    public var runStep: SidebarRun? = nil
}

public struct SidebarRun: Sendable, Hashable {
    public let runID: String
    public let skill: String
    public let step: String
    public let stepName: String
    public let urgency: StepUrgency
    public let firstQuestion: String?

    init(_ run: StepRun, step: StepNode) {
        runID = run.id
        skill = run.skill
        self.step = step.id
        stepName = step.title
        urgency = run.urgency
        firstQuestion = run.firstQuestion
    }
}

public struct SidebarRunDestination: Sendable, Equatable {
    public let workspaceID: String
    public let run: StepRun
}

public struct StatusCount: Sendable, Hashable {
    public let status: AgentStatus
    public let count: Int
}

public struct SidebarSelection: Sendable, Equatable {
    public let rowID: String
    public let workspaceID: String
    public let chatID: SwarmSessionID?
    public let agentSessionID: SwarmSessionID?
    public let agentID: SwarmAgentID?
}

public struct SidebarSection: Sendable, Hashable, Identifiable {
    public enum Kind: Sendable, Hashable {
        case pinned
        /// Keyed by `ProjectNode.path`, which stays the same when a folder becomes a git repo.
        case project(path: String)
    }

    public var collapseID: String {
        switch kind {
        case .pinned: "pinned"
        case .project(let path): path
        }
    }

    public let kind: Kind
    /// Unique even when two projects share a path, such as a bare clone kept as a folder.
    public let id: String
    public let title: String
    /// The most urgent status of the rows; the header shows it while collapsed.
    public let status: AgentStatus?
    public let rows: [SidebarRow]

    /// A project section's id, which also names the project for its header's "+".
    public static func id(of project: ProjectNode) -> String { "project:\(project.id)" }

    init(kind: Kind, id: String, title: String, rows: [SidebarRow]) {
        self.kind = kind
        self.id = id
        self.title = title
        self.status = AgentStatus.aggregate(rows.compactMap(\.status))
        self.rows = rows
    }
}

public enum SidebarRows {
    public static func runWorkspaces(_ entries: [WorkspaceEntry], navigation: WorkspaceNavigation) -> [String] {
        entries.filter {
            !$0.workspace.missing && !$0.workspace.isRemoved && !navigation.archived.contains($0.id)
                && (!navigation.isCollapsed($0.id) || navigation.pinned.contains($0.id))
        }.map(\.id)
    }

    private static func orderedRuns(_ runs: [StepRun]) -> [StepRun] {
        runs.filter { !$0.closed }.sorted {
            if $0.urgency != $1.urgency { return $0.urgency > $1.urgency }
            if $0.lastActivity != $1.lastActivity { return ($0.lastActivity ?? .distantPast) > ($1.lastActivity ?? .distantPast) }
            return $0.id < $1.id
        }
    }

    private static func runReference(_ run: StepRun) -> SidebarRun? {
        guard let step = run.steps.max(by: { $0.urgency < $1.urgency }) else { return nil }
        return SidebarRun(run, step: step)
    }

    public static func runSummary(_ runs: [StepRun]) -> String? {
        let open = orderedRuns(runs)
        let waiting = open.count { $0.urgency == .waiting }
        if waiting > 0 { return waiting == 1 ? "1 run waits" : "\(waiting) runs waiting" }
        guard let run = open.first, let reference = runReference(run) else { return nil }
        return "\(reference.skill) · \(reference.stepName)"
    }

    /// A step names an agent, not a session. Resolve only inside its workspace (ADR 0051).
    public static func chat(
        for step: StepNode, in entry: WorkspaceEntry, agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> SwarmProjectSession? {
        guard case .active(let name) = step.state, !name.isEmpty else { return nil }
        return entry.chats.filter { chat in
            chat.sessions.contains { session in
                (agentsBySession[session.id] ?? []).contains { $0.id.rawValue == name }
            }
        }.sorted {
            $0.lastActivity == $1.lastActivity ? chatID($0) < chatID($1) : $0.lastActivity > $1.lastActivity
        }.first
    }

    public static func runDestination(
        for rowID: String, in entries: [WorkspaceEntry], runsByWorkspace: [String: [StepRun]],
        agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> SidebarRunDestination? {
        guard let selection = selection(for: rowID, in: entries, agentsBySession: agentsBySession),
              selection.agentID == nil, let entry = entries.first(where: { $0.id == selection.workspaceID }) else { return nil }
        let runs = orderedRuns(runsByWorkspace[entry.id] ?? [])
        let run: StepRun?
        if let chatID = selection.chatID {
            run = runs.first { run in
                run.steps.contains { chat(for: $0, in: entry, agentsBySession: agentsBySession)?.id == chatID }
            }
        } else {
            run = runs.first
        }
        return run.map { SidebarRunDestination(workspaceID: entry.id, run: $0) }
    }

    /// Pinned, then one section per project in tree order, even an empty one, so its "+" stays
    /// reachable. The archive view shows only projects with archived rows. Rows keep the
    /// workspace order, so a status change sets a row's glyph but never moves the row.
    public static func sections(
        projects: [ProjectNode], workspaces: [WorkspaceEntry], navigation: WorkspaceNavigation,
        search: String, showingArchive: Bool, now: Int,
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:], expandedLists: Set<String> = [],
        runsByWorkspace: [String: [StepRun]] = [:]
    ) -> [SidebarSection] {
        let entriesByProject = Dictionary(grouping: workspaces, by: { $0.project.id }).mapValues { entries in
            Dictionary(entries.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        }
        let ordered = projects.flatMap { project in
            SessionsTree.ordered(project.workspaces, order: navigation.workspaceOrder[project.path] ?? [],
                                 mainPath: project.mainWorkspacePath, hubPath: project.path)
                .compactMap { entriesByProject[project.id]?[$0.path] }
        }
        let visible = ordered.filter { navigation.matches(search, entry: $0) }
        var sections: [SidebarSection] = []
        if !showingArchive {
            // Titles once for the whole list; a per-row scan made this quadratic in workspaces.
            let idsByTitle = navigation.idsByTitle(workspaces)
            let pinned = visible.filter {
                navigation.pinned.contains($0.id) && !navigation.archived.contains($0.id)
            }.flatMap {
                rows($0, idsByTitle: idsByTitle, navigation: navigation, now: now,
                     agentsBySession: agentsBySession, expandedLists: expandedLists, runs: runsByWorkspace[$0.id] ?? [])
            }
            if !pinned.isEmpty { sections.append(SidebarSection(kind: .pinned, id: "pinned", title: "Pinned", rows: pinned)) }
        }
        let names = Dictionary(grouping: projects.map { navigation.projectTitle(for: $0) }, by: { $0 }).mapValues(\.count)
        for project in projects {
            let members = workspaces.filter { $0.project.id == project.id }
            let idsByTitle = navigation.idsByTitle(members, inProject: true)
            let rows = visible.filter { entry in
                entry.project.id == project.id && (showingArchive
                    ? navigation.archived.contains(entry.id)
                    : !navigation.pinned.contains(entry.id) && !navigation.archived.contains(entry.id))
            }.flatMap {
                rows($0, idsByTitle: idsByTitle, navigation: navigation, now: now, inProject: true,
                     agentsBySession: agentsBySession, expandedLists: expandedLists, runs: runsByWorkspace[$0.id] ?? [])
            }
            if rows.isEmpty, showingArchive || !search.isEmpty { continue }
            let parent = URL(fileURLWithPath: project.path).deletingLastPathComponent().lastPathComponent
            let name = navigation.projectTitle(for: project)
            let title = names[name, default: 0] > 1 && !parent.isEmpty
                ? "\(name) — \(parent)" : name
            sections.append(SidebarSection(
                kind: .project(path: project.path), id: SidebarSection.id(of: project), title: title, rows: rows
            ))
        }
        return sections
    }

    /// A continuation can change the current session id without changing this row's identity.
    public static func chatID(_ chat: SwarmProjectSession) -> String {
        "chat:\(ChatTitle.key(chat))"
    }

    /// Resolve against the current tree, so a stale row cannot select a different chat or agent.
    public static func selection(
        for id: String, in workspaces: [WorkspaceEntry], agentsBySession: [SwarmSessionID: [SwarmAgent]]
    ) -> SidebarSelection? {
        for entry in workspaces {
            if id == entry.id {
                return SidebarSelection(rowID: id, workspaceID: entry.id, chatID: nil, agentSessionID: nil, agentID: nil)
            }
            for chat in entry.chats {
                if id == chatID(chat) {
                    return SidebarSelection(rowID: id, workspaceID: entry.id, chatID: chat.id, agentSessionID: nil, agentID: nil)
                }
                for session in chat.sessions {
                    for agent in agentsBySession[session.id] ?? []
                    where agent.id != SwarmPanePolicy.chair && agent.id.rawValue != session.chairID?.rawValue {
                        if id == "child:\(session.id.rawValue)/\(agent.id.rawValue)" {
                            return SidebarSelection(rowID: id, workspaceID: entry.id, chatID: chat.id,
                                                    agentSessionID: session.id, agentID: agent.id)
                        }
                    }
                }
            }
        }
        return nil
    }

    /// When a fold hides the selection, highlight its nearest visible parent.
    public static func selectedID(
        in rows: [SidebarRow], workspace: String?, chat: SwarmProjectSession?, child: SidebarSelection?
    ) -> String? {
        if let child, child.chatID == chat?.id, rows.contains(where: { $0.id == child.rowID }) {
            return child.rowID
        }
        if let chat {
            let id = chatID(chat)
            if rows.contains(where: { $0.id == id }) { return id }
        }
        return workspace
    }

    private static func rows(
        _ entry: WorkspaceEntry, idsByTitle: [String: [String]], navigation: WorkspaceNavigation,
        now: Int, inProject: Bool = false, agentsBySession: [SwarmSessionID: [SwarmAgent]],
        expandedLists: Set<String>, runs: [StepRun]
    ) -> [SidebarRow] {
        var result = [row(entry, idsByTitle: idsByTitle, navigation: navigation, now: now,
                          inProject: inProject, agentsBySession: agentsBySession, runs: runs)]
        guard !navigation.isCollapsed(entry.id) else { return result }
        var stepsByChat: [SwarmSessionID: SidebarRun] = [:]
        for run in orderedRuns(runs) {
            for step in run.steps {
                if let chat = self.chat(for: step, in: entry, agentsBySession: agentsBySession), stepsByChat[chat.id] == nil {
                    stepsByChat[chat.id] = SidebarRun(run, step: step)
                }
            }
        }
        let chats = entry.chats.sorted {
            $0.lastActivity == $1.lastActivity ? chatID($0) < chatID($1) : $0.lastActivity > $1.lastActivity
        }
        let shown = expandedLists.contains(entry.id) ? chats.count : min(5, chats.count)
        for chat in chats.prefix(shown) {
            let id = chatID(chat)
            let expanded = !navigation.isCollapsed(id)
            let children = chat.sessions.flatMap { session in
                (agentsBySession[session.id] ?? [])
                    .filter { $0.id != SwarmPanePolicy.chair && $0.id.rawValue != session.chairID?.rawValue }
                    .sorted { $0.id < $1.id }
                    .map { (session.id, $0) }
            }
            let agents = agentsBySession[chat.id]
            let status = expanded && !children.isEmpty
                ? agents?.first { $0.id == SwarmPanePolicy.chair || $0.id.rawValue == chat.session.chairID?.rawValue }?.status
                : AgentStatus.aggregate((agents ?? []).map(\.status) + children.map { $0.1.status }) ?? chat.status
            let counts = chat.statusCounts.map { StatusCount(status: $0.key, count: $0.value) }
                .sorted { $0.status.urgency > $1.status.urgency }
            let presentation = SessionRowPresentation.make(
                ChatRow(session: chat, workspace: entry.workspace.name, workspacePath: entry.id), now: now,
                appName: navigation.chatNames[ChatTitle.key(chat)]
            )
            let waiting = children.filter { $0.1.status == .waiting }.count
            let childrenLabel = children.count == 1 ? "1 agent" : "\(children.count) agents"
            result.append(SidebarRow(
                id: id, kind: .chat, depth: 1, parentID: entry.id, expanded: expanded,
                hasChildren: !children.isEmpty,
                childrenSummary: children.isEmpty ? nil : childrenLabel + (waiting == 0 ? "" : " · \(waiting) waiting"),
                title: presentation.title, detail: "", status: status, counts: counts, age: presentation.age,
                help: "\(presentation.title)\n\(entry.id)", pinned: navigation.pinned.contains(entry.id),
                archived: navigation.archived.contains(entry.id), missing: false, newChatEnabled: false,
                runStep: stepsByChat[chat.id]
            ))
            if expanded {
                result += children.map { sessionID, agent in
                    SidebarRow(
                        id: "child:\(sessionID.rawValue)/\(agent.id.rawValue)", kind: .child, depth: 2,
                        parentID: id, expanded: false, hasChildren: false, childrenSummary: nil,
                        title: agent.id.rawValue, detail: "· \(agent.role)", status: agent.status, counts: [], age: nil,
                        help: "\(agent.id.rawValue) · \(agent.role)", pinned: navigation.pinned.contains(entry.id),
                        archived: navigation.archived.contains(entry.id), missing: false, newChatEnabled: false
                    )
                }
            }
        }
        if shown < chats.count {
            result.append(SidebarRow(
                id: "more:\(entry.id)", kind: .more, depth: 1, parentID: entry.id,
                expanded: false, hasChildren: false, childrenSummary: nil,
                title: "… \(chats.count - shown) more", detail: "", status: nil, counts: [], age: nil,
                help: "Show all chats", pinned: navigation.pinned.contains(entry.id),
                archived: navigation.archived.contains(entry.id), missing: false, newChatEnabled: false
            ))
        }
        return result
    }

    static func row(
        _ entry: WorkspaceEntry, idsByTitle: [String: [String]],
        navigation: WorkspaceNavigation, now: Int, inProject: Bool = false,
        agentsBySession: [SwarmSessionID: [SwarmAgent]] = [:], runs: [StepRun] = []
    ) -> SidebarRow {
        let age = entry.chats.max { $0.lastActivity < $1.lastActivity }.map {
            SessionRowPresentation.make(
                ChatRow(session: $0, workspace: entry.workspace.name, workspacePath: entry.id), now: now
            ).age
        }
        let counts = entry.statusCounts
            .map { StatusCount(status: $0.key, count: $0.value) }
            .sorted { $0.status.urgency > $1.status.urgency }
        return SidebarRow(
            id: entry.id, kind: .workspace, depth: 0, parentID: nil,
            expanded: !navigation.isCollapsed(entry.id), hasChildren: !entry.chats.isEmpty, childrenSummary: nil,
            title: entry.workspace.isRemoved
                ? entry.workspace.name : navigation.title(for: entry, inProject: inProject),
            detail: ([navigation.detail(for: entry, idsByTitle: idsByTitle, inProject: inProject)]
                + (entry.workspace.missing ? ["folder missing"] : [])
                + (entry.workspace.mark.map { [$0.rawValue] } ?? [])).joined(separator: " · "),
            status: AgentStatus.aggregate(entry.chats.compactMap(\.status) + orderedRuns(runs).map {
                switch $0.urgency {
                case .waiting: AgentStatus.waiting
                case .blocked, .stale: AgentStatus.failed
                case .active: AgentStatus.working
                case .open: AgentStatus.ended
                case .done: AgentStatus.done
                }
            } + entry.chats.flatMap {
                $0.sessions.flatMap { (agentsBySession[$0.id] ?? []).map(\.status) }
            }), counts: counts, age: age,
            help: "\(navigation.projectTitle(for: entry.project)) · \(entry.workspace.name)\n\(entry.id)",
            pinned: navigation.pinned.contains(entry.id),
            archived: navigation.archived.contains(entry.id),
            missing: entry.workspace.missing, newChatEnabled: entry.workspace.canStartChat,
            run: orderedRuns(runs).first.flatMap(runReference), runSummary: runSummary(runs)
        )
    }
}
