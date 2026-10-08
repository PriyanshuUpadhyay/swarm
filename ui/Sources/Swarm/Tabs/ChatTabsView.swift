import SwiftUI
import SwarmCore

struct ChatTabActions {
    var select: (String) -> Void
    var newChat: () -> Void
    var close: (String) -> Void
    var hide: (String) -> Void
    var move: (String, String) -> Bool
    var archive: (String) -> Void
    var rename: (String) -> Void
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
                        ForEach(tabs) { tab in
                            draggableTab(tab)
                                .id(tab.id)
                                .onGeometryChange(for: Double.self) { geometry in
                                    Double(geometry.frame(in: .named("tabViewport")).maxX)
                                } action: { trailingEdges[tab.id] = $0 }
                        }
                    }
                }
                .scrollIndicators(.hidden)
                .coordinateSpace(name: "tabViewport")
                .onGeometryChange(for: Double.self) { Double($0.size.width) } action: { viewportWidth = $0 }
                .onAppear { proxy.scrollTo(selectedID) }
                .onChange(of: selectedID) { _, id in proxy.scrollTo(id) }
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
    }

    @ViewBuilder private func draggableTab(_ tab: ChatTab) -> some View {
        let view = ChatTabView(tab: tab, selected: tab.id == selectedID, canStartChat: canStartChat, actions: actions)
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
            // A start is never cut in half, so a pending tab has no archive or close.
            if tab.pending == nil {
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
                archiveButton
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
                Button("New chat here", action: actions.newChat)
                    .disabled(!canStartChat)
                Button("Close chat") { actions.close(tab.id) }.disabled(!tab.canClose)
                Button("Archive chat") { actions.archive(tab.id) }
            }
        }
    }

    private var archiveButton: some View {
        Button { actions.archive(tab.id) } label: {
            Image(systemName: "archivebox")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(DesignTokens.Spacing.s)
                .contentShape(Rectangle())
                .fixedSize()
        }
        .help("Archive chat")
        .accessibilityLabel("Archive \(tab.title)")
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
