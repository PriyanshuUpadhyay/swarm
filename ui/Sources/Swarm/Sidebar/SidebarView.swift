import SwiftUI
import SwarmCore

enum WorkspaceSidebarMode: String, CaseIterable {
    case workspaces = "Workspaces", files = "Files", changes = "Changes", pullRequest = "PR", usage = "Usage"

    var isDetails: Bool { self == .changes || self == .pullRequest || self == .usage }

    var symbol: String {
        switch self {
        case .workspaces: "square.stack.3d.up"
        case .files: "folder"
        case .changes: "arrow.triangle.branch"
        case .pullRequest: "arrow.triangle.pull"
        case .usage: "chart.pie"
        }
    }
}

struct SidebarActions {
    var selectMode: (WorkspaceSidebarMode) -> Void
    var select: (String) -> Void
    var home: () -> Void
    var create: () -> Void
    /// Opens the command palette, which is also the workspace search.
    var openPalette: () -> Void
    var toggleArchive: () -> Void
    var openProject: () -> Void
    var createProject: () -> Void
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
                Button("Create workspace", systemImage: "plus", action: actions.create)
                Button("Command palette", systemImage: "magnifyingglass", action: actions.openPalette)
                Spacer()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .padding(.horizontal, DesignTokens.Spacing.m)
            .padding(.vertical, DesignTokens.Spacing.xs)
            List(selection: Binding(get: { selectedID }, set: { $0.map(actions.select) })) {
                ForEach(sections) { section in
                    Section(section.title) {
                        if section.rows.isEmpty {
                            Text("No workspaces")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        ForEach(section.rows) { row in
                            SidebarRowView(row: row)
                                .tag(row.id)
                                .contextMenu { menu(for: row) }
                        }
                    }
                }
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            HStack {
                Button(action: actions.toggleArchive) {
                    Label(showingArchive ? "Workspaces" : "Archived", systemImage: "clock.arrow.circlepath")
                }
                Spacer()
                Menu {
                    Button("Open Project…", action: actions.openProject)
                    Button("Create Project…", action: actions.createProject)
                } label: {
                    Image(systemName: "folder.badge.plus")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .accessibilityLabel("Add project")
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .padding(DesignTokens.Spacing.m)
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

/// One line: status glyph, name, and branch; the last-activity age turns into agent counts by
/// status while the pointer is over the row.
private struct SidebarRowView: View {
    let row: SidebarRow
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
        }
        .frame(minHeight: DesignTokens.Size.row)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .help(row.help)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.detail.isEmpty ? row.title : "\(row.title), \(row.detail)")
        .accessibilityValue(accessibilityValue)
    }

    private var accessibilityValue: String {
        let counts = row.counts.map { "\($0.count) \(StatusGlyph.title($0.status).lowercased())" }
        return (row.status.map { [StatusGlyph.title($0)] } ?? []).joined() + (counts.isEmpty ? "" : "; " + counts.joined(separator: ", "))
    }
}
