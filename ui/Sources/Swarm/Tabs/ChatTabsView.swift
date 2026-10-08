import SwiftUI
import SwarmCore

struct ChatTabActions {
    var select: (String) -> Void
    var selectChildren: (String) -> Void
    var newChat: () -> Void
    var end: (String) -> Void
    var hide: (String) -> Void
    var move: (String, String) -> Bool
    var archive: (String) -> Void
    var rename: (String) -> Void
    var group: (TabStrip.Grouping) -> Bool
}

/// The workspace's chats as tabs: status glyph, title, and provider badge. A tab fits its title
/// between 120 and 220 pt.
struct ChatTabsView: View {
    let workspaceTitle: String?
    let tabs: [ChatTab]
    let selectedID: String
    let canStartChat: Bool
    let actions: ChatTabActions
    @State private var trailingEdges: [String: Double] = [:]
    @State private var viewportWidth: Double = 0
    @State private var groupEditor: TabGroupEditorTarget?

    private var runs: [ChatTab.Run] { ChatTab.runs(tabs) }
    private var groups: [TabGroup] { runs.compactMap(\.group) }

    private var overflow: [ChatTab] {
        ChatTab.overflow(tabs, trailingEdges: trailingEdges, viewportWidth: viewportWidth)
    }

    var body: some View {
        HStack(spacing: DesignTokens.Spacing.s) {
            if let workspaceTitle {
                Text(workspaceTitle)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(maxWidth: DesignTokens.Size.tabMaxWidth, alignment: .leading)
                    .padding(.leading, DesignTokens.Spacing.m)
            }
            ScrollViewReader { proxy in
                ScrollView(.horizontal) {
                    HStack(spacing: DesignTokens.Spacing.xs) {
                        ForEach(runs) { run in
                            HStack(spacing: DesignTokens.Spacing.xs) {
                                if let group = run.group { groupChip(group, tabs: run.tabs) }
                                if run.group?.folded != true {
                                    ForEach(run.tabs) { tab in
                                        draggableTab(tab)
                                            .id(tab.id)
                                            .onGeometryChange(for: Double.self) { geometry in
                                                Double(geometry.frame(in: .named("tabViewport")).maxX)
                                            } action: { trailingEdges[tab.id] = $0 }
                                    }
                                }
                            }
                            .overlay(alignment: .bottom) {
                                if let group = run.group {
                                    Rectangle().fill(group.color.tint).frame(height: DesignTokens.Size.focusRing)
                                }
                            }
                        }
                    }
                }
                .scrollIndicators(.hidden)
                .coordinateSpace(name: "tabViewport")
                .onGeometryChange(for: Double.self) { Double($0.size.width) } action: { viewportWidth = $0 }
                .onAppear { proxy.scrollTo(selectedID) }
                .onChange(of: selectedID) { _, id in
                    if let group = tabs.first(where: { $0.id == id })?.group, group.folded {
                        _ = actions.group(.fold(group.id, false))
                    }
                    proxy.scrollTo(id)
                }
            }
            Menu("More tabs", systemImage: "chevron.down") {
                ForEach(overflow) { tab in
                    Button(tab.title) { actions.select(tab.id) }
                }
            }
            .labelStyle(.iconOnly)
            .menuStyle(.borderlessButton)
            .fixedSize()
            .disabled(overflow.isEmpty)
            .help("Tabs past the right edge")
            Button("New chat in this workspace", systemImage: "plus", action: actions.newChat)
                .disabled(!canStartChat)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help("New chat in this workspace")
                .padding(.trailing, DesignTokens.Spacing.m)
        }
        .padding(.vertical, DesignTokens.Spacing.xs)
        .sheet(item: $groupEditor) { target in
            TabGroupEditor(target: target) { name, color in
                if let group = target.group {
                    _ = actions.group(.rename(group.id, to: name))
                    _ = actions.group(.recolor(group.id, to: color))
                } else if let tab = target.tab {
                    _ = actions.group(.new(id: UUID().uuidString, name: name, color: color, tab: tab))
                }
                groupEditor = nil
            }
        }
    }

    private func groupChip(_ group: TabGroup, tabs: [ChatTab]) -> some View {
        Button {
            _ = actions.group(.fold(group.id, !group.folded))
        } label: {
            HStack(spacing: DesignTokens.Spacing.xs) {
                Text(group.name).lineLimit(1)
                if group.folded { Text("\(tabs.count)") }
                Image(systemName: group.folded ? "chevron.right" : "chevron.down").font(.caption2)
            }
            .padding(DesignTokens.Spacing.s)
            .foregroundStyle(group.color.tint)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(group.name), \(tabs.count) tabs, \(group.folded ? "folded" : "expanded")")
        .help(group.folded ? "Expand group" : "Fold group")
        .dropDestination(for: String.self) { keys, _ in
            guard keys.count == 1, let key = keys.first else { return false }
            return actions.group(.add(key, to: group.id))
        }
        .onGeometryChange(for: Double.self) { geometry in
            Double(geometry.frame(in: .named("tabViewport")).maxX)
        } action: { edge in
            if group.folded { for tab in tabs { trailingEdges[tab.id] = edge } }
        }
        .contextMenu {
            Button("Rename group…") { groupEditor = .init(group: group) }
            Menu("Color") {
                ForEach(TabGroupColor.allCases, id: \.self) { color in
                    Button(color.rawValue.capitalized) { _ = actions.group(.recolor(group.id, to: color)) }
                }
            }
            Button(group.folded ? "Expand group" : "Fold group") {
                _ = actions.group(.fold(group.id, !group.folded))
            }
            Button("Delete group") { _ = actions.group(.delete(group.id)) }
        }
    }

    @ViewBuilder private func draggableTab(_ tab: ChatTab) -> some View {
        let view = ChatTabView(tab: tab, selected: tab.id == selectedID, canStartChat: canStartChat,
                               actions: actions, groups: groups, newGroup: { groupEditor = .init(tab: $0) })
        if tab.pending == nil {
            view.draggable(tab.id)
                .dropDestination(for: String.self) { keys, _ in
                    guard keys.count == 1, let key = keys.first else { return false }
                    return actions.move(key, tab.id)
                }
        } else {
            view
        }
    }
}

private struct ChatTabView: View {
    let tab: ChatTab
    let selected: Bool
    let canStartChat: Bool
    let actions: ChatTabActions
    let groups: [TabGroup]
    let newGroup: (String) -> Void
    @State private var hovered = false

    var body: some View {
        HStack(spacing: 0) {
            Button { actions.select(tab.id) } label: {
                HStack(spacing: DesignTokens.Spacing.xs) {
                    switch tab.pending {
                    case .starting:
                        ProgressView().controlSize(.mini).accessibilityLabel("Starting")
                    case .closing:
                        ProgressView().controlSize(.mini).accessibilityLabel("Closing")
                    case .failed:
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.caption).foregroundStyle(.red).accessibilityLabel("Failed")
                    case nil:
                        EmptyView()
                    }
                    if tab.pending != nil {
                        Text(tab.title).lineLimit(1).truncationMode(.tail)
                    } else {
                        ForEach(Array(tab.fields.enumerated()), id: \.offset) { _, value in
                            if value.field == .provider, let badge = tab.badge {
                                Text(badge).font(.caption2).foregroundStyle(.secondary).fixedSize()
                            } else {
                                RowFieldLabel(value: value)
                            }
                        }
                    }
                }
                .padding(.leading, DesignTokens.Spacing.s)
                .padding(.vertical, DesignTokens.Spacing.s)
                .contentShape(Rectangle())
            }
            .accessibilityAddTraits(selected ? .isSelected : [])
            .help(tab.title)
            if let children = tab.children {
                Button { actions.selectChildren(tab.id) } label: {
                    Text(children.text)
                        .font(.caption)
                        .foregroundStyle(children.waiting > 0 ? .orange : .secondary)
                        .padding(.horizontal, DesignTokens.Spacing.xs)
                        .fixedSize()
                }
                .help(children.waiting > 0 ? "Show first waiting child" : "Show chat")
                .accessibilityLabel("\(tab.title), \(children.count) agents, \(children.waiting) waiting")
            }
            // A start is never cut in half, so a pending tab has no archive or close.
            if tab.canHide {
                Button { actions.hide(tab.id) } label: {
                    Image(systemName: "xmark")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding(DesignTokens.Spacing.s)
                        .contentShape(Rectangle())
                        .fixedSize()
                }
                .help("Close tab")
                .accessibilityLabel("Close tab \(tab.title)")
                .opacity(selected || hovered ? 1 : 0)
                .allowsHitTesting(selected || hovered)
                .accessibilityHidden(!selected && !hovered)
            }
        }
        .frame(minWidth: DesignTokens.Size.tabMinWidth, alignment: .leading)
        .modifier(CappedWidth(max: DesignTokens.Size.tabMaxWidth))
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .background(selected ? DesignTokens.selectionFill : .clear, in: .rect(cornerRadius: DesignTokens.Radius.control))
        .contextMenu {
            if tab.pending == nil {
                Button("Rename chat…") { actions.rename(tab.id) }
                Menu("Move to group") {
                    ForEach(groups) { group in
                        Button(group.name) { _ = actions.group(.add(tab.id, to: group.id)) }
                            .disabled(tab.group?.id == group.id)
                    }
                    Button("New group…") { newGroup(tab.id) }
                    if tab.group != nil {
                        Button("Remove from group") { _ = actions.group(.remove(tab.id)) }
                    }
                }
                Button("New chat here", action: actions.newChat)
                    .disabled(!canStartChat)
                Divider()
                Button("Close tab") { actions.hide(tab.id) }
                Button("End chat…") { actions.end(tab.id) }.disabled(!tab.canClose)
                Button("Archive chat") { actions.archive(tab.id) }
            }
        }
    }


}

/// A horizontal scroll view offers its content unlimited width, so a max-width frame alone lets a
/// long title overflow and clip. This offers the tab at most `max`, so its title truncates.
private struct CappedWidth: ViewModifier {
    let max: CGFloat

    func body(content: Content) -> some View {
        CappedWidthLayout(max: max) { content }
    }
}

private struct CappedWidthLayout: Layout {
    let max: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        subviews.first?.sizeThatFits(capped(proposal)) ?? .zero
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, proposal: ProposedViewSize(bounds.size))
    }

    private func capped(_ proposal: ProposedViewSize) -> ProposedViewSize {
        ProposedViewSize(width: min(proposal.width ?? max, max), height: proposal.height)
    }
}

private struct TabGroupEditorTarget: Identifiable {
    var id = UUID()
    var group: TabGroup?
    var tab: String?
}

private struct TabGroupEditor: View {
    let target: TabGroupEditorTarget
    let save: (String, TabGroupColor) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var color: TabGroupColor

    init(target: TabGroupEditorTarget, save: @escaping (String, TabGroupColor) -> Void) {
        self.target = target
        self.save = save
        _name = State(initialValue: target.group?.name ?? "")
        _color = State(initialValue: target.group?.color ?? .blue)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text(target.group == nil ? "New group" : "Rename group").font(.title2)
            TextField("Name", text: $name).textFieldStyle(.roundedBorder)
            Picker("Color", selection: $color) {
                ForEach(TabGroupColor.allCases, id: \.self) { choice in
                    Text(choice.rawValue.capitalized).tag(choice)
                }
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Save") { save(name, color) }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(DesignTokens.Spacing.l)
        .frame(minWidth: DesignTokens.Size.tabMaxWidth)
    }
}

private extension TabGroupColor {
    var tint: Color {
        switch self {
        case .grey: .gray
        case .blue: .blue
        case .green: .green
        case .yellow: .yellow
        case .orange: .orange
        case .red: .red
        case .purple: .purple
        case .pink: .pink
        }
    }
}
