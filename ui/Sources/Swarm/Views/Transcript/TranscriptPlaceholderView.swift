import SwiftUI

/// What a loaded but empty transcript says.
///
/// A blank white rectangle above the composer reads as a rendering failure, and it is the first
/// thing a new workspace shows: the session exists, the setup script is still running, and nothing
/// has happened yet. Saying which of those it is costs one sentence.
struct TranscriptPlaceholderView: View {
    var isRunningSetup: Bool

    /// What to say instead of "Nothing here yet", for a conversation whose empty state is a
    /// different sentence.
    ///
    /// Ask Swarm had its own `EmptyStateView` laid over the transcript in a `ZStack`, and the
    /// transcript went on drawing this one underneath: two headings and two paragraphs on top of
    /// each other. One pane shows one empty state, so the caller replaces the words rather than
    /// covering them up. Setting up still wins over both, because it is the more urgent thing to
    /// say and it is temporary.
    var emptyState: TranscriptEmptyState?
    /// Sends one of the empty state's suggestions as the conversation's first message.
    var onSuggestion: ((String) -> Void)?

    var body: some View {
        if isRunningSetup {
            EmptyStateView(
                glyph: "gearshape.2",
                title: "Setting up the workspace",
                message: "The setup script is still running. Ask for something now and it goes as soon as that finishes."
            )
        } else if let emptyState, !emptyState.suggestions.isEmpty {
            suggesting(emptyState)
        } else if let emptyState {
            EmptyStateView(glyph: emptyState.glyph, title: emptyState.title, message: emptyState.message)
        } else {
            EmptyStateView(
                glyph: "text.alignleft",
                title: "Nothing here yet",
                message: "Ask for something below and the agent's work shows up here."
            )
        }
    }

    /// The empty state with prompts under it, one to a line, centred under the words.
    ///
    /// Buttons rather than text to copy, and they send rather than fill the composer: every
    /// suggestion is a finished question, so a click that only fills the field is a click and a
    /// Return where one click would do.
    private func suggesting(_ state: TranscriptEmptyState) -> some View {
        VStack(spacing: Metrics.spacingWide) {
            EmptyStateView(glyph: state.glyph, title: state.title, message: state.message)
                .fixedSize(horizontal: false, vertical: true)

            if let heading = state.suggestionsHeading {
                Text(heading)
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textSecondary)
            }

            VStack(spacing: Metrics.spacingSmall) {
                ForEach(state.suggestions, id: \.self) { prompt in
                    SuggestionChip(prompt: prompt) { onSuggestion?(prompt) }
                }
            }
            .disabled(onSuggestion == nil)
        }
        .padding(.horizontal, Metrics.gutter)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// One suggested prompt, as a capsule.
private struct SuggestionChip: View {
    var prompt: String
    var action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Text(prompt)
                .font(Typo.label)
                .foregroundStyle(Palette.textPrimary)
                .lineLimit(1)
                .padding(.horizontal, Metrics.gutter)
                .padding(.vertical, Metrics.spacing)
                .background(isHovered ? Palette.hover : Palette.surface, in: Capsule())
                .overlay { Capsule().strokeBorder(Palette.border, lineWidth: Metrics.outline) }
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHoverChange { isHovered = $0 }
        .help("Ask this")
    }
}

/// The words an empty transcript shows, for the panes that have their own.
struct TranscriptEmptyState: Equatable {
    var glyph: String
    var title: String
    var message: String
    /// Said above the suggestions, when there are any.
    var suggestionsHeading: String?
    /// Prompts offered as buttons under the words. Empty for a plain empty state.
    var suggestions: [String] = []
}
