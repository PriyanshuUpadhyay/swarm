import SwiftUI

/// One line above an agent terminal after its `swarm attach` process exits.
struct SwarmAgentPaneReattachStrip: View {
    var onReattach: () -> Void

    var body: some View {
        HStack(spacing: Metrics.spacing) {
            Image(systemName: "arrow.clockwise")
                .imageScale(.small)
                .foregroundStyle(Palette.textSecondary)
                .frame(width: Metrics.glyph, height: Metrics.glyph)
                .accessibilityHidden(true)

            Text("Live pane detached")
                .font(Typo.caption)
                .foregroundStyle(Palette.textSecondary)
                .lineLimit(1)

            Spacer(minLength: 0)

            Button("Reattach", action: onReattach)
                .controlSize(.small)
        }
        .padding(.horizontal, Metrics.gutter)
        .padding(.vertical, Metrics.spacing)
        .frame(maxWidth: .infinity)
        .background(Palette.surface)
        .overlay(alignment: .bottom) { Hairline() }
        .transition(.move(edge: .top).combined(with: .opacity))
    }
}
