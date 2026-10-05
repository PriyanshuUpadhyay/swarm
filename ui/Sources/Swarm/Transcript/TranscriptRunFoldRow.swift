import SwiftUI
import SwarmCore

/// A run of steps between two prose rows, folded into one line (ADR 0047): chevron, status, the
/// step counts, failures in the failed color, the newest step while one runs, and the total time.
/// Open, it shows each step's own row under the line.
struct TranscriptRunFoldRow<Child: View>: View {
    let rows: [TranscriptRow]
    @Binding var expanded: Bool
    @ViewBuilder let child: (TranscriptRow) -> Child

    var body: some View {
        let summary = ToolRunFold.summary(of: rows)
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            Button { expanded.toggle() } label: {
                HStack(spacing: DesignTokens.Spacing.s) {
                    Image(systemName: expanded ? "chevron.down" : "chevron.forward")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .frame(width: DesignTokens.Size.glyphSlot)
                    TranscriptStatusGlyph(state: summary.failed > 0 ? .failed : summary.isRunning ? .waiting : .finished)
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
                    Spacer(minLength: DesignTokens.Spacing.s)
                    if let duration = summary.duration {
                        Text(verbatim: TranscriptToolActivity.durationLabel(duration))
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, DesignTokens.Spacing.xs)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(summary.accessibilityLabel)
            .accessibilityValue(expanded ? "Expanded" : "Collapsed")
            .accessibilityHint(expanded ? "Hides the steps" : "Shows the steps")
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("transcript-tool-run-fold")
            if expanded {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                    ForEach(rows) { row in child(row) }
                }
                .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.s)
            }
        }
    }
}
