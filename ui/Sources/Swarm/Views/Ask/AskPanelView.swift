import SwiftUI
import SwarmCore

/// Ask Swarm as a card in front of the window: the conversations down the side, the selected one
/// beside them.
///
/// Drawn by `SearchPanelWindowOverlay`, on the same ground and behind the same dim as the search
/// panel, for the reason given at the head of that file. See `AskPanelLayout` for why Ask is a
/// panel at all.
///
/// **Opaque, where the search card is glass.** The search card is a short list of rows. This one
/// holds a transcript, and a transcript over regular glass carries whatever is behind the card
/// into every line of it.
struct AskPanelView: View {
    let app: AppModel
    var width: CGFloat
    var height: CGFloat

    private static let corner: CGFloat = 12

    var body: some View {
        HStack(spacing: 0) {
            // No rail until there is a conversation to list. An empty column with only a `+` in it
            // offered a new conversation beside the one already waiting to be typed into.
            if !app.ask.listedSessions.isEmpty {
                AskConversationRail()
                    .frame(width: AskPanelLayout.railWidth)

                Hairline(axis: .vertical)
            }

            VStack(spacing: 0) {
                header
                Hairline()
                AskConversationContent()
            }
            .background(Palette.windowBackground)
        }
        .frame(width: width, height: height)
        .clipShape(RoundedRectangle(cornerRadius: Self.corner))
        .overlay {
            RoundedRectangle(cornerRadius: Self.corner)
                .strokeBorder(Palette.border, lineWidth: Metrics.outline)
        }
        .elevation(.lifted)
        // Escape reaches here only when nothing inside wanted it: a composer with a menu open, or
        // a turn to interrupt, answers it first.
        .onExitCommand { AskPanelModel.shared.close() }
        // Going anywhere else in the window is leaving the panel. The sidebar stays usable under
        // the dim only through the keyboard and the menu bar, and both move the selection.
        .onChange(of: app.selection) { _, _ in AskPanelModel.shared.close() }
    }

    private var header: some View {
        HStack(spacing: Metrics.spacingSmall) {
            Text(title)
                .font(Typo.labelEmphasis)
                .foregroundStyle(Palette.textPrimary)
                .lineLimit(1)
                .truncationMode(.tail)

            Spacer(minLength: Metrics.spacing)

            headerButton("Close", systemImage: "xmark") {
                AskPanelModel.shared.close()
            }
        }
        .padding(.leading, Metrics.gutter)
        .padding(.trailing, Metrics.spacingSmall)
        .frame(height: Metrics.barHeight + Metrics.spacingWide)
    }

    /// The conversation's own name, and nothing for one that has not earned one yet, rather than
    /// "Ask Swarm" over a panel that is already Ask Swarm.
    private var title: String {
        guard let session = app.ask.session, !app.ask.isBlank(session) else { return "" }
        let title = app.ask.title(for: session)
        return title == AskConversation.title ? "" : title
    }

    private func headerButton(
        _ title: String, systemImage: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .labelStyle(.iconOnly)
                .font(Typo.label)
                .frame(width: Metrics.rowHeight, height: Metrics.rowHeight)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.textSecondary)
        .help(title)
    }
}
