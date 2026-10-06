import SwiftUI
import SwarmCore

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
    var createProject: () -> Void
    var toggleCollapsed: (String) -> Void
    var newChat: (String) -> Void
    var togglePin: (String) -> Void
    var rename: (String) -> Void
    var archive: (String) -> Void
    var restore: (String) -> Void
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
                    ForEach(sections) { section in
                        if case .project(let path) = section.kind {
                            // The archive view lists every archived row, with no collapse.
                            let expanded = showingArchive || !collapsed.contains(path)
                            Section {
                                if expanded { rows(section.rows) }
                            } header: {
                                ProjectHeader(
                                    title: section.title, expanded: expanded,
                                    status: expanded ? nil : section.status,
                                    toggle: showingArchive ? nil : { actions.toggleCollapsed(path) },
                                    newWorkspace: showingArchive ? nil : { actions.newWorkspace(section.id) }
                                )
                            }
                        } else {
                            Section(section.title) { rows(section.rows) }
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
            SidebarRowView(
                row: row, selected: row.id == selectedID,
                newChat: row.archived ? nil : { actions.newChat(row.id) }
            )
            .tag(row.id)
            .contextMenu { menu(for: row) }
        }
    }

    @ViewBuilder
    private func menu(for row: SidebarRow) -> some View {
        if row.archived {
            Button("Restore workspace") { actions.restore(row.id) }
        } else {
            Button("New chat") { actions.newChat(row.id) }
            Button(row.pinned ? "Unpin workspace" : "Pin workspace") { actions.togglePin(row.id) }
            Button("Rename workspace…") { actions.rename(row.id) }
            Button("Archive workspace") { actions.archive(row.id) }
        }
    }
}

/// A project's header: a chevron and name that collapse it, its most urgent status while
/// collapsed, and a "+" that makes a workspace in it.
private struct ProjectHeader: View {
    let title: String
    let expanded: Bool
    let status: AgentStatus?
    /// Nil in the archive view, which has no collapse.
    let toggle: (() -> Void)?
    let newWorkspace: (() -> Void)?

    var body: some View {
        HStack(spacing: DesignTokens.Spacing.xs) {
            if let toggle {
                Button(action: toggle) {
                    HStack(spacing: DesignTokens.Spacing.xs) {
                        Image(systemName: expanded ? "chevron.down" : "chevron.forward")
                            .font(.caption2.weight(.semibold))
                            .frame(width: DesignTokens.Size.glyphSlot)
                        Text(title).lineLimit(1).truncationMode(.middle)
                        if let status { StatusGlyph(status: status) }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(title)
                .accessibilityAddTraits(.isHeader)
                .accessibilityValue(expanded ? "expanded" : (["collapsed"] + (status.map { [StatusGlyph.title($0)] } ?? [])).joined(separator: ", "))
            } else {
                Text(title).lineLimit(1).truncationMode(.middle)
                    .accessibilityAddTraits(.isHeader)
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
        }
    }
}

/// One line: status glyph, name, and branch; the last-activity age turns into agent counts by
/// status while the pointer is over the row.
private struct SidebarRowView: View {
    let row: SidebarRow
    let selected: Bool
    let newChat: (() -> Void)?
    @State private var hovering = false

    var body: some View {
        HStack(spacing: DesignTokens.Spacing.s) {
            Group {
                if let status = row.status { StatusGlyph(status: status) }
            }
            .frame(width: DesignTokens.Size.glyphSlot)
            Text(row.title).lineLimit(1).layoutPriority(1)
            Text(row.detail)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: DesignTokens.Spacing.xs)
            if hovering, !row.counts.isEmpty {
                HStack(spacing: DesignTokens.Spacing.s) {
                    ForEach(row.counts, id: \.status) { count in
                        HStack(spacing: DesignTokens.Spacing.xxs) {
                            StatusGlyph(status: count.status)
                            Text("\(count.count)").monospacedDigit()
                        }
                    }
                }
                .font(.caption)
            } else if let age = row.age {
                Text(age).font(.caption).foregroundStyle(.secondary)
            }
            if let newChat, hovering || selected {
                Button("New chat in \(row.title)", systemImage: "plus", action: newChat)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help("New chat in \(row.title)")
            }
        }
        .frame(minHeight: DesignTokens.Size.row)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .help(row.help)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.detail.isEmpty ? row.title : "\(row.title), \(row.detail)")
        .accessibilityValue(accessibilityValue)
        .accessibilityActions {
            if let newChat { Button("New chat", action: newChat) }
        }
    }

    private var accessibilityValue: String {
        let counts = row.counts.map { "\($0.count) \(StatusGlyph.title($0.status).lowercased())" }
        return (row.status.map { [StatusGlyph.title($0)] } ?? []).joined() + (counts.isEmpty ? "" : "; " + counts.joined(separator: ", "))
    }
}
