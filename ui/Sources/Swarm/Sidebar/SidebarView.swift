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
    var togglePin: (String) -> Void
    var pinWorkspace: (String) -> Bool
    var moveWorkspace: (String, String) -> Bool
    var rename: (String) -> Void
    var renameChat: (String) -> Void
    var renameProject: (String) -> Void
    var archive: (String) -> Void
    var archiveChat: (String) -> Void
    var restore: (String) -> Void
    var removeProject: (String) -> Void
    var pruneWorktree: (String) -> Void
    var showRun: (String) -> Void
}

/// The sidebar chrome: a view switcher over the workspace list, or over the workspace details that
/// the caller passes in. It gets plain rows and closures, and owns no app state.
struct SidebarView<Details: View>: View {
    let mode: WorkspaceSidebarMode
    let sections: [SidebarSection]
    let collapsed: Set<String>
    let loaded: Bool
    let selectedID: String?
    let showingArchive: Bool
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
            .padding(DesignTokens.Spacing.s)
            ZStack {
                workspaceList.retainedVisibility(mode == .workspaces)
                details().retainedVisibility(mode != .workspaces)
            }
        }
        .chromeSurface()
    }

    private var workspaceList: some View {
        VStack(spacing: 0) {
            HStack(spacing: DesignTokens.Spacing.m) {
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
            .padding(.horizontal, DesignTokens.Spacing.m)
            .padding(.vertical, DesignTokens.Spacing.xs)
            if sections.isEmpty {
                // A failed load shows its error on Home; the list stays blank until a tree loads.
                if loaded { emptyList } else { Spacer() }
            } else {
                List(selection: Binding(get: { selectedID }, set: { $0.map(actions.select) })) {
                    if !showingArchive, !sections.contains(where: { $0.kind == .pinned }) {
                        Section {} header: { pinnedHeader(status: nil) }
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
                                if !collapsed.contains("pinned") { rows(section.rows) }
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
                    Label(showingArchive ? "Workspaces" : "Archived", systemImage: "clock.arrow.circlepath")
                }
                Spacer()
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .padding(DesignTokens.Spacing.m)
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard let folder = SidebarDrop.folder(in: urls) else { return false }
            actions.importFolder(folder)
            return true
        }
    }

    @ViewBuilder
    private var emptyList: some View {
        VStack(spacing: DesignTokens.Spacing.m) {
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
            toggle: row.hasChildren ? { actions.toggleCollapsed(row.id) } : nil,
            showRun: { actions.showRun(row.id) }
        )
        .tag(row.id)
        .contextMenu { menu(for: row) }
    }

    private func pinnedHeader(status: AgentStatus?) -> some View {
        let expanded = !collapsed.contains("pinned")
        return HStack(spacing: DesignTokens.Spacing.xs) {
            Button { actions.toggleCollapsed("pinned") } label: {
                HStack(spacing: DesignTokens.Spacing.xs) {
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
            Spacer(minLength: DesignTokens.Spacing.xs)
        }
        .contentShape(Rectangle())
        .dropDestination(for: WorkspaceDrag.self) { items, _ in
            guard items.count == 1, let source = items.first else { return false }
            return actions.pinWorkspace(source.path)
        }
    }

    @ViewBuilder
    private func menu(for row: SidebarRow) -> some View {
        if row.kind == .chat, !row.archived {
            Button("Rename chat…") { actions.renameChat(row.id) }
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
            Button(row.pinned ? "Unpin workspace" : "Pin workspace") { actions.togglePin(row.id) }
            Button("Rename workspace…") { actions.rename(row.id) }
            Button("Archive workspace") { actions.archive(row.id) }
        }
        if row.missing {
            Button("Prune worktree") { actions.pruneWorktree(row.id) }
        }
    }
}

/// A project's header: a chevron and name that collapse it, its most urgent status while
/// collapsed, and a "+" that makes a workspace in it.
private struct ProjectHeader: View {
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
        HStack(spacing: DesignTokens.Spacing.xs) {
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
        HStack(spacing: DesignTokens.Spacing.xs) {
            if let toggle {
                Button(action: toggle) {
                    HStack(spacing: DesignTokens.Spacing.xs) {
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
            Spacer(minLength: DesignTokens.Spacing.xs)
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
    let row: SidebarRow
    let selected: Bool
    let newChat: (() -> Void)?
    let toggle: (() -> Void)?
    @State private var hovering = false
    var showRun: (() -> Void)? = nil

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
        .padding(.leading, CGFloat(row.depth) * DesignTokens.Spacing.l)
        .opacity(row.kind == .child && (row.status == .done || row.status == .ended) ? DesignTokens.endedPaneOpacity : 1)
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
        HStack(spacing: DesignTokens.Spacing.s) {
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
                    Button(action: { showRun?() }) { RowFieldLabel(value: value) }
                        .buttonStyle(.plain)
                        .help(row.run?.firstQuestion ?? row.runStep?.firstQuestion ?? value.text)
                        .accessibilityLabel("Show run, \(value.text)")
                } else if value.field == .children, row.kind == .chat, let toggle {
                    Button(action: toggle) {
                        HStack(spacing: DesignTokens.Spacing.xs) { disclosure; RowFieldLabel(value: value) }
                    }
                    .buttonStyle(.plain)
                    .accessibilityValue(row.expanded ? "expanded" : "collapsed")
                } else if value.field == .age, hovering, !row.counts.isEmpty {
                    HStack(spacing: DesignTokens.Spacing.s) {
                        ForEach(row.counts, id: \.status) { count in
                            HStack(spacing: DesignTokens.Spacing.xxs) {
                                StatusGlyph(status: count.status)
                                Text("\(count.count)").monospacedDigit()
                            }
                        }
                    }
                    .font(.caption)
                } else {
                    RowFieldLabel(value: value)
                }
            }
            Spacer(minLength: DesignTokens.Spacing.xs)
            if first, row.kind == .workspace, !row.archived, hovering {
                Image(systemName: "line.3.horizontal")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .help("Drag to reorder or pin workspace")
                    .accessibilityHidden(true)
            }
            if first, let newChat, hovering || selected {
                Button("New chat in \(row.title)", systemImage: "plus", action: newChat)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help("New chat in \(row.title)")
            }
        }
        .frame(minHeight: first ? DesignTokens.Size.row : nil)
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
    let value: RowFieldValue

    var body: some View {
        if let status = value.status {
            StatusGlyph(status: status).fixedSize()
        } else if value.field == .unread {
            Image(systemName: "circle.fill").font(.caption2).foregroundStyle(.tint)
                .accessibilityLabel("Unread")
        } else {
            Text(verbatim: value.text).lineLimit(1)
                .truncationMode(value.field == .title ? .tail : .middle)
                .foregroundStyle(value.field == .title ? .primary : .secondary)
                .font(value.field == .title ? .body : .caption)
                .layoutPriority(value.field == .title ? 1 : 0)
        }
    }
}
