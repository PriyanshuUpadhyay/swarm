import SwiftUI
import SwarmCore

/// One line for a record the reader has no case for, with the record itself behind it.
///
/// It is drawn in the shape of `AgentErrorRowView` and says far less, because it knows far less.
/// The title is the provider's own word for the record, never a guess, and the body is the stored
/// line laid out. A row like this is how a new CLI feature reaches the chat on the day it ships
/// rather than on the day somebody writes a card for it. See `OpaqueRecord`.
struct OpaqueRecordRowView: View {
    var record: OpaqueRecord
    var isExpanded: Bool
    var onToggle: () -> Void

    @State private var isHovered = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ExpandableRowHeader(isExpanded: isExpanded, onToggle: onToggle) {
                header
            }

            if isExpanded {
                DetailCodeBlock(text: record.detail, copyTitle: "Copy record")
                    .padding(.leading, TranscriptLayout.inset)
                    .padding(.top, 4)
            }
        }
        .modifier(ExpandableRow(isHovered: isHovered))
        .onHover { isHovered = $0 }
    }

    private var header: some View {
        HStack(spacing: TranscriptLayout.glyphGap) {
            TranscriptGlyph(symbol: "curlybraces", tint: Palette.textTertiary)

            Text(record.title)
                .font(Typo.label)
                .foregroundStyle(Palette.textSecondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .transcriptLabelColumn(record.title, font: Typo.label)

            if !record.summary.isEmpty {
                Text(record.summary)
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textTertiary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }

            Spacer(minLength: 0)
        }
    }
}
