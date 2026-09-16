import SwiftUI
import BloomCore

/// Every open Ask conversation, down the leading edge of the panel, newest first.
///
/// Rows rather than tabs, because a strip of titles across a card this size truncates every one
/// of them after a word, so here they
/// are rows: the title in full, and the mark the sidebar's own rows wear when a turn is running or
/// waiting on you, so a conversation left running behind another one still says so.
struct AskConversationRail: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // No heading. The panel is already what you opened Ask Bloom to see, so its name over
            // the rail and over the conversation said the same thing twice. New stays.
            HStack(spacing: Metrics.spacing) {
                Spacer(minLength: Metrics.spacingSmall)

                Button { Task { await app.ask.newConversation() } } label: {
                    Label("New conversation", systemImage: "plus")
                        .labelStyle(.iconOnly)
                        .frame(width: Metrics.rowHeight, height: Metrics.rowHeight)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.textSecondary)
                .help("New Ask Bloom conversation")
            }
            .padding(.leading, Metrics.gutter)
            .padding(.trailing, Metrics.spacingSmall)
            .frame(height: Metrics.barHeight + Metrics.spacingWide)

            ScrollView {
                LazyVStack(spacing: Metrics.spacingHair) {
                    ForEach(AskPanelLayout.railOrder(app.ask.listedSessions), id: \.id) { chat in
                        AskConversationRailRow(
                            title: app.ask.title(for: chat),
                            status: app.ask.status(for: chat.id),
                            isSelected: app.ask.selectedID == chat.id,
                            onSelect: { Task { await app.ask.select(chat.id) } },
                            onClose: { app.ask.requestClose(chat.id) }
                        )
                    }
                }
                .padding(.horizontal, Metrics.spacingSmall)
                .padding(.bottom, Metrics.spacingSmall)
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Palette.surfaceSunken)
    }
}

private struct AskConversationRailRow: View {
    var title: String
    var status: WorkspaceStatus?
    var isSelected: Bool
    var onSelect: () -> Void
    var onClose: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: onSelect) {
            HStack(spacing: Metrics.spacing) {
                Text(title)
                    .font(Typo.label)
                    .foregroundStyle(Palette.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.tail)

                Spacer(minLength: Metrics.spacingSmall)

                if let status {
                    WorkspaceStatusGlyph(status: status)
                }
            }
            .padding(.horizontal, Metrics.spacingWide)
            .frame(height: Metrics.rowHeight)
            .contentShape(Rectangle())
            .background(
                isSelected ? Palette.selected : (isHovered ? Palette.hover : .clear),
                in: RoundedRectangle(cornerRadius: Metrics.corner)
            )
        }
        .buttonStyle(.plain)
        .onHoverChange { isHovered = $0 }
        .contextMenu {
            Button("Close Conversation", action: onClose)
        }
        .accessibilityValue(status?.label ?? "")
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}
