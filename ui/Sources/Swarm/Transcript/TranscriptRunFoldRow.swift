import SwiftUI
import SwarmCore

/// A finished run of tools in an ended turn, folded into one line: chevron, status, "N tools", the
/// tool names, and the total time. Open, it shows each tool row under the line.
struct TranscriptRunFoldRow<Child: View>: View {
    let rows: [TranscriptRow]
    @Binding var expanded: Bool
    @ViewBuilder let child: (TranscriptRow) -> Child

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            Button { expanded.toggle() } label: {
                HStack(spacing: DesignTokens.Spacing.s) {
                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                        .frame(width: DesignTokens.Size.glyphSlot)
                    TranscriptStatusGlyph(state: .finished)
                    Text(verbatim: "\(rows.count) tools").fontWeight(.semibold).lineLimit(1).layoutPriority(1)
                    Text(verbatim: ToolRunFold.summary(of: rows))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: DesignTokens.Spacing.s)
                    if let duration = ToolRunFold.totalDuration(of: rows) {
                        Text(verbatim: TranscriptToolActivity.durationLabel(duration))
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.vertical, DesignTokens.Spacing.xs)
            .accessibilityLabel(accessibilityLabel)
            .accessibilityHint(expanded ? "Hides the tools" : "Shows the tools")
            .accessibilityIdentifier("transcript-tool-run-fold")
            if expanded {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                    ForEach(rows) { row in child(row) }
                }
                .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.s)
            }
        }
    }

    /// "5 tools, Read 2, Grep, Edit, finished".
    private var accessibilityLabel: String {
        let names = ToolRunFold.nameCounts(rows).map { $0.count > 1 ? "\($0.name) \($0.count)" : $0.name }
        return (["\(rows.count) tools"] + names + ["finished"]).joined(separator: ", ")
    }
}
