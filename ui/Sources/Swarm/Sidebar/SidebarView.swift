import SwiftUI
import SwarmCore
import CoreTransferable
import UniformTypeIdentifiers

enum WorkspaceSidebarMode: String, CaseIterable {
    case workspaces = "Workspaces", files = "Files", changes = "Changes", pullRequest = "PR", usage = "Usage", runs = "Runs"

    var isDetails: Bool { self == .changes || self == .pullRequest || self == .usage }

    var symbol: String {
        switch self {
        case .workspaces: "square.stack.3d.up"
        case .files: "folder"
        case .changes: "arrow.triangle.branch"
        case .pullRequest: "arrow.triangle.pull"
        case .usage: "chart.pie"
        case .runs: "point.3.connected.trianglepath.dotted"
        }
    }
}

struct SidebarActions {
    var selectMode: (WorkspaceSidebarMode) -> Void
    var select: (String) -> Void
    var home: () -> Void
    /// Makes a workspace in the project of this section id.
    var newWorkspace: (String) -> Void
    /// Opens the command palette, which is also the workspace search.
    var openPalette: () -> Void
    var toggleArchive: () -> Void
    var importProject: () -> Void
    var importFolder: (URL) -> Void
    var createProject: () -> Void
    var toggleCollapsed: (String) -> Void
    var expandList: (String) -> Void
    var newChat: (String) -> Void
    var newChatProfiles: [NewChatMenu.Row]
    var refreshNewChatProfiles: () -> Void
    var newChatAs: (String, String) -> Void
    var unpinWorkspace: (String) -> Void
    var pinWorkspace: (String) -> Bool
    var moveWorkspace: (String, String) -> Bool
    var workspaceMoveTarget: (String, Int) -> String?
    var rename: (String) -> Void
    var renameChat: (String) -> Void
    var openChatInNewWindow: (String) -> Void
    var renameProject: (String) -> Void
    var archive: (String) -> Void
    var endChat: (String) -> Void
    var archiveChat: (String) -> Void
    var deleteWorkspace: (String) -> Void
    var canDeleteWorkspace: (String) -> Bool
    var restore: (String) -> Void
    var removeProject: (String) -> Void
    var pruneWorktree: (String) -> Void
    var showRun: (String) -> Void
}

/// The sidebar chrome: a view switcher over the workspace list, or over the workspace details that
/// the caller passes in. It gets plain rows and closures, and owns no app state.
struct SidebarView<Details: View>: View {
    @Environment(\.designTokens) private var tokens
    let mode: WorkspaceSidebarMode
    let sections: [SidebarSection]
    let collapsed: Set<String>
    let loaded: Bool
    let selectedID: String?
    let showingArchive: Bool
    let archivedStatus: AgentStatus?
    let actions: SidebarActions
    @ViewBuilder let details: () -> Details

    var body: some View {
        VStack(spacing: 0) {
            Picker("Sidebar view", selection: Binding(get: { mode }, set: actions.selectMode)) {
                ForEach(WorkspaceSidebarMode.allCases, id: \.self) { mode in
                    Label(mode.rawValue, systemImage: mode.symbol)
                        .labelStyle(.iconOnly)
                        .help(mode.rawValue)
                        .tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .controlSize(.small)
            .padding(tokens.spacing.s)
            ZStack {
                workspaceList.retainedVisibility(mode == .workspaces)
                details().retainedVisibility(mode != .workspaces)
            }
        }
        .chromeSurface()
    }

    private var workspaceList: some View {
        VStack(spacing: 0) {
            HStack(spacing: tokens.spacing.m) {
                Button("Home", systemImage: "house", action: actions.home)
                Menu {
                    Button("Create Project…", action: actions.createProject)
                    Button("Import Project…", action: actions.importProject)
                } label: {
                    Image(systemName: "plus")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .accessibilityLabel("Add project")
                .help("Add project")
                Button("Command palette", systemImage: "magnifyingglass", action: actions.openPalette)
                Spacer()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .padding(.horizontal, tokens.spacing.m)
            .padding(.vertical, tokens.spacing.xs)
            if sections.isEmpty {
                // A failed load shows its error on Home; the list stays blank until a tree loads.
                if loaded { emptyList } else { Spacer() }
            } else {
                List(selection: Binding(get: { selectedID }, set: { $0.map(actions.select) })) {
                    if !showingArchive, !sections.contains(where: { $0.kind == .pinned }) {
                        // The empty header remains a target for the first workspace pin.
                        Section {} header: { pinnedHeader(status: nil, hasRows: false) }
                    }
                    ForEach(sections) { section in
                        if case .project = section.kind {
                            // The archive view lists every archived row, with no collapse.
                            let expanded = showingArchive || !collapsed.contains(section.collapseID)
                            Section {
                                if expanded { rows(section.rows) }
                            } header: {
                                ProjectHeader(
                                    title: section.title, fields: section.fields, expanded: expanded,
                                    status: expanded ? nil : section.status,
                                    toggle: showingArchive ? nil : { actions.toggleCollapsed(section.collapseID) },
                                    newWorkspace: showingArchive ? nil : { actions.newWorkspace(section.id) },
                                    renameProject: { actions.renameProject(section.id) },
                                    removeProject: { actions.removeProject(section.id) }
                                )
                            }
                        } else {
                            Section {
                                if !collapsed.contains(section.collapseID) { rows(section.rows) }
                            } header: {
                                pinnedHeader(status: section.status)
                            }
                        }
                    }
                }
                .listStyle(.sidebar)
                .scrollContentBackground(.hidden)
            }
            HStack {
                Button(action: actions.toggleArchive) {
                    HStack(spacing: tokens.spacing.xs) {
                        Label(showingArchive ? "Workspaces" : "Archived", systemImage: "clock.arrow.circlepath")
                        if !showingArchive, let archivedStatus { StatusGlyph(status: archivedStatus) }
                    }
                }
                Spacer()
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .padding(tokens.spacing.m)
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard let folder = SidebarDrop.folder(in: urls) else { return false }
            actions.importFolder(folder)
            return true
        }
    }

    @ViewBuilder
    private var emptyList: some View {
        VStack(spacing: tokens.spacing.m) {
            if showingArchive {
                Text("No archived workspaces").foregroundStyle(.secondary)
            } else {
                Text("No projects yet").foregroundStyle(.secondary)
                Button("Create Project…", action: actions.createProject)
                Button("Import Project…", action: actions.importProject)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func rows(_ rows: [SidebarRow]) -> some View {
        ForEach(rows) { row in
            if row.kind == .more, let workspace = row.parentID {
                Button { actions.expandList(workspace) } label: {
                    SidebarRowView(row: row, selected: false, newChat: nil, toggle: nil)
                }
                .buttonStyle(.plain)
            } else {
                if row.kind == .workspace, !showingArchive {
                    rowView(row)
                        .draggable(WorkspaceDrag(path: row.id))
                        .dropDestination(for: WorkspaceDrag.self) { items, _ in
                            guard items.count == 1, let source = items.first else { return false }
                            return actions.moveWorkspace(source.path, row.id)
                        }
                } else {
                    rowView(row)
                }
            }
        }
    }

    private func rowView(_ row: SidebarRow) -> some View {
        SidebarRowView(
            row: row, selected: row.id == selectedID,
            newChat: row.archived || !row.newChatEnabled ? nil : { actions.newChat(row.id) },
            newChatProfiles: actions.newChatProfiles,
            refreshNewChatProfiles: actions.refreshNewChatProfiles,
            newChatAs: { actions.newChatAs(row.id, $0) },
            toggle: row.hasChildren ? { actions.toggleCollapsed(row.id) } : nil,
            showRun: { actions.showRun(row.id) }
        )
        .tag(row.id)
        .contextMenu { menu(for: row) }
        .accessibilityActions {
            if row.kind == .workspace, !row.archived {
                workspaceMoveButtons(for: row, showDisabled: false)
            }
        }
    }

    private func pinnedHeader(status: AgentStatus?, hasRows: Bool = true) -> some View {
        let expanded = !collapsed.contains("pinned")
        return HStack(spacing: tokens.spacing.xs) {
            if hasRows {
                Button { actions.toggleCollapsed("pinned") } label: {
                    HStack(spacing: tokens.spacing.xs) {
                        Image(systemName: expanded ? "chevron.down" : "chevron.forward")
                            .font(.caption2.weight(.semibold))
                            .frame(width: DesignTokens.Size.glyphSlot)
                        Text("Pinned")
                        if !expanded, let status { StatusGlyph(status: status) }
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Pinned")
                .accessibilityAddTraits(.isHeader)
                .accessibilityValue(expanded ? "expanded" : "collapsed")
            } else {
                Text("Pinned")
                    .padding(.leading, DesignTokens.Size.glyphSlot + tokens.spacing.xs)
                    .accessibilityAddTraits(.isHeader)
            }
            Spacer(minLength: tokens.spacing.xs)
        }
        .contentShape(Rectangle())
        .dropDestination(for: WorkspaceDrag.self) { items, _ in
            guard items.count == 1, let source = items.first else { return false }
            return pinWorkspace(source.path)
        }
    }

    private func pinWorkspace(_ path: String) -> Bool {
        guard actions.pinWorkspace(path) else { return false }
        if collapsed.contains("pinned") { actions.toggleCollapsed("pinned") }
        return true
    }

    @ViewBuilder
    private func menu(for row: SidebarRow) -> some View {
        if row.kind == .chat, !row.archived {
            Button("Open in New Window") { actions.openChatInNewWindow(row.id) }
            Button("Rename chat…") { actions.renameChat(row.id) }
            Button("End chat…") { actions.endChat(row.id) }
            Button("Archive chat") { actions.archiveChat(row.id) }
        } else if row.kind == .workspace {
            workspaceMenu(for: row)
        }
    }

    @ViewBuilder
    private func workspaceMenu(for row: SidebarRow) -> some View {
        if row.archived {
            Button("Restore workspace") { actions.restore(row.id) }
        } else {
            Button("New chat") { actions.newChat(row.id) }
                .disabled(!row.newChatEnabled)
            Button(row.pinned ? "Unpin workspace" : "Pin workspace") {
                if row.pinned { actions.unpinWorkspace(row.id) }
                else { _ = pinWorkspace(row.id) }
            }
            Button("Rename workspace…") { actions.rename(row.id) }
            workspaceMoveButtons(for: row, showDisabled: true)
            Button("Archive workspace") { actions.archive(row.id) }
        }
        if actions.canDeleteWorkspace(row.id) {
            Button("Delete workspace…", role: .destructive) { actions.deleteWorkspace(row.id) }
        }
        if row.missing {
            Button("Prune worktree…", role: .destructive) { actions.pruneWorktree(row.id) }
        }
    }

    @ViewBuilder
    private func workspaceMoveButtons(for row: SidebarRow, showDisabled: Bool) -> some View {
        ForEach([-1, 1], id: \.self) { offset in
            let target = actions.workspaceMoveTarget(row.id, offset)
            if target != nil || showDisabled {
                Button(offset < 0 ? "Move Up" : "Move Down") {
                    if let target { _ = actions.moveWorkspace(row.id, target) }
                }
                .disabled(target == nil)
            }
        }
    }
}

/// A project's header: a chevron and name that collapse it, its most urgent status while
/// collapsed, and a "+" that makes a workspace in it.
private struct ProjectHeader: View {
    @Environment(\.designTokens) private var tokens
    let title: String
    let fields: [RowFieldValue]
    let expanded: Bool
    let status: AgentStatus?
    /// Nil in the archive view, which has no collapse.
    let toggle: (() -> Void)?
    let newWorkspace: (() -> Void)?
    let renameProject: () -> Void
    let removeProject: () -> Void

    private var fieldContent: some View {
        HStack(spacing: tokens.spacing.xs) {
            ForEach(Array(fields.enumerated()), id: \.offset) { _, value in
                if value.field == .status {
                    if let status { StatusGlyph(status: status) }
                } else {
                    RowFieldLabel(value: value)
                }
            }
        }
    }

    var body: some View {
        HStack(spacing: tokens.spacing.xs) {
            if let toggle {
                Button(action: toggle) {
                    HStack(spacing: tokens.spacing.xs) {
                        Image(systemName: expanded ? "chevron.down" : "chevron.forward")
                            .font(.caption2.weight(.semibold))
                            .frame(width: DesignTokens.Size.glyphSlot)
                        fieldContent
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(title)
                .accessibilityAddTraits(.isHeader)
                .accessibilityValue(expanded ? "expanded" : (["collapsed"] + (status.map { [StatusGlyph.title($0)] } ?? [])).joined(separator: ", "))
            } else {
                fieldContent.accessibilityAddTraits(.isHeader)
            }
            Spacer(minLength: tokens.spacing.xs)
            if let newWorkspace {
                Button("New workspace in \(title)", systemImage: "plus", action: newWorkspace)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help("New workspace in \(title)")
            }
        }
        .contextMenu {
            if let newWorkspace { Button("New Workspace…", action: newWorkspace) }
            Button("Rename Project…", action: renameProject)
            Button("Remove Project…", action: removeProject)
        }
    }
}

/// The selected fields keep their order. Hovering replaces an age with status counts.
private struct SidebarRowView: View {
    @Environment(\.designTokens) private var tokens
    let row: SidebarRow
    let selected: Bool
    let newChat: (() -> Void)?
    var newChatProfiles: [NewChatMenu.Row] = []
    var refreshNewChatProfiles: () -> Void = {}
    var newChatAs: (String) -> Void = { _ in }
    let toggle: (() -> Void)?
    @State private var hovering = false
    var showRun: (() -> Void)? = nil

    private var dimmed: Bool { row.dimmed }

    private var lines: [[RowFieldValue]] {
        var result: [[RowFieldValue]] = [[]]
        for value in row.fields {
            if (value.field == .steps || (value.field == .children && row.kind == .chat)), !result[result.count - 1].isEmpty {
                result.append([])
            }
            result[result.count - 1].append(value)
        }
        return result
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.enumerated()), id: \.offset) { index, values in
                line(values, first: index == 0)
            }
        }
        .padding(.leading, CGFloat(row.depth) * tokens.spacing.l)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .help(row.help)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(row.detail.isEmpty ? row.title : "\(row.title), \(row.detail)")
        .accessibilityValue(accessibilityValue)
        .accessibilityActions {
            if let toggle { Button(row.expanded ? "Collapse" : "Expand", action: toggle) }
            if let newChat { Button("New chat", action: newChat) }
        }
    }

    private var disclosure: some View {
        Image(systemName: row.expanded ? "chevron.down" : "chevron.forward")
            .font(.caption2.weight(.semibold))
            .frame(width: DesignTokens.Size.glyphSlot)
    }

    private func line(_ values: [RowFieldValue], first: Bool) -> some View {
        HStack(spacing: tokens.spacing.s) {
            Group {
                if first, (row.kind == .workspace || row.kind == .chat), let toggle {
                    Button(action: toggle) { disclosure }
                        .buttonStyle(.borderless)
                        .accessibilityLabel(row.expanded ? "Collapse" : "Expand")
                } else {
                    Color.clear
                }
            }
            .frame(width: DesignTokens.Size.glyphSlot)
            ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                if value.field == .steps {
                    Button(action: { showRun?() }) { RowFieldLabel(value: value, dimmed: dimmed) }
                        .buttonStyle(.plain)
                        .help(row.run?.firstQuestion ?? row.runStep?.firstQuestion ?? value.text)
                        .accessibilityLabel("Show run, \(value.text)")
                } else if value.field == .children, row.kind == .chat, let toggle {
                    Button(action: toggle) {
                        HStack(spacing: tokens.spacing.xs) { disclosure; RowFieldLabel(value: value, dimmed: dimmed) }
                    }
                    .buttonStyle(.plain)
                    .accessibilityValue(row.expanded ? "expanded" : "collapsed")
                } else if value.field == .age, hovering, !row.counts.isEmpty {
                    HStack(spacing: tokens.spacing.s) {
                        ForEach(row.counts, id: \.status) { count in
                            HStack(spacing: tokens.spacing.xxs) {
                                StatusGlyph(status: count.status)
                                Text("\(count.count)").monospacedDigit()
                            }
                        }
                    }
                    .font(.caption)
                } else {
                    RowFieldLabel(value: value, dimmed: dimmed)
                }
            }
            Spacer(minLength: tokens.spacing.xs)
            if first, row.kind == .workspace, !row.archived, hovering {
                Image(systemName: "line.3.horizontal")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .help("Drag to reorder or pin workspace")
                    .accessibilityHidden(true)
            }
            if first, let newChat, hovering || selected {
                NewChatProfileMenu(
                    rows: newChatProfiles, refresh: refreshNewChatProfiles, start: newChatAs, primaryAction: newChat
                ) { Label("New chat in \(row.title)", systemImage: "plus") }
                .labelStyle(.iconOnly)
                .menuStyle(.borderlessButton)
                .foregroundStyle(.secondary)
                .help("New chat in \(row.title)")
            }
        }
        .frame(minHeight: first ? tokens.row : nil)
    }

    private var accessibilityValue: String {
        let counts = row.counts.map { "\($0.count) \(StatusGlyph.title($0.status).lowercased())" }
        return (row.status.map { [StatusGlyph.title($0)] } ?? []).joined() + (counts.isEmpty ? "" : "; " + counts.joined(separator: ", "))
    }
}

private struct WorkspaceDrag: Codable, Transferable {
    let path: String

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: UTType(exportedAs: "io.github.priyanshuupadhyay.swarm.workspace"))
    }
}

struct RowFieldLabel: View {
    @Environment(\.designTokens) private var tokens
    let value: RowFieldValue
    var dimmed = false

    var body: some View {
        if let status = value.status {
            StatusGlyph(status: status).fixedSize()
                .opacity(dimmed ? DesignTokens.endedPaneOpacity : 1)
        } else if value.field == .unread {
            Image(systemName: "circle.fill").font(.caption2).foregroundStyle(.tint)
                .accessibilityLabel("Unread")
                .opacity(dimmed ? DesignTokens.endedPaneOpacity : 1)
        } else if value.field == .provider {
            HStack(spacing: tokens.spacing.xs) {
                ForEach(value.text.components(separatedBy: " · "), id: \.self) { provider in
                    ProviderMark(provider: provider)
                }
            }
            .foregroundStyle(dimmed ? .tertiary : .secondary)
            .fixedSize()
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(value.text)
        } else {
            Text(verbatim: value.text).lineLimit(1)
                .truncationMode(value.field == .title ? .tail : .middle)
                .foregroundStyle(value.field == .title ? (dimmed ? .secondary : .primary) : (dimmed ? .tertiary : .secondary))
                .font(value.field == .title ? .body : .caption)
                .layoutPriority(value.field == .title ? 1 : 0)
        }
    }
}
