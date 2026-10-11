import SwiftUI
import SwarmCore

/// A run of steps between two prose rows, folded into one line (ADR 0047): chevron, status, the
/// step counts, failures in the failed color, the waiting step while one runs, and the total time.
/// Open, the transcript list draws each step's own row under the line (`ToolRunFold.lines`).
struct TranscriptRunFoldRow: View {
    @Environment(\.designTokens) private var tokens
    let rows: [TranscriptRow]
    @Binding var expanded: Bool

    var body: some View {
        let summary = ToolRunFold.summary(of: rows)
        Button { expanded.toggle() } label: {
            HStack(spacing: tokens.spacing.s) {
                Image(systemName: expanded ? "chevron.down" : "chevron.forward")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: DesignTokens.Size.glyphSlot)
                TranscriptStatusGlyph(state: summary.state)
                Text(verbatim: summary.stepsText).fontWeight(.semibold).lineLimit(1).layoutPriority(1)
                if summary.failed > 0 {
                    Text(verbatim: "· \(summary.failed) failed")
                        .foregroundStyle(DesignTokens.color(.failed))
                        .lineLimit(1)
                        .layoutPriority(1)
                }
                if summary.isRunning, !summary.latestTitle.isEmpty {
                    Text(verbatim: "· now: \(summary.latestTitle)")
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                Spacer(minLength: tokens.spacing.s)
                if let duration = summary.duration {
                    Text(verbatim: TranscriptToolActivity.durationLabel(duration))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.vertical, tokens.spacing.xs)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(summary.accessibilityLabel)
        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
        .accessibilityHint(expanded ? "Hides the steps" : "Shows the steps")
        .accessibilityIdentifier("transcript-tool-run-fold")
    }
}
