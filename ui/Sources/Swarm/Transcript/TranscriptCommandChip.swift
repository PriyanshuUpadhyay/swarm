import SwiftUI
import SwarmCore

/// A slash command the owner typed: a mono capsule with its name, then its arguments, a closed
/// skill body, and the command's local output. It is drawn inside the user's bubble.
struct TranscriptCommandChipView: View {
    @Environment(\.designTokens) private var tokens
    let chip: TranscriptCommandChip
    var revealForSearch = false
    @State private var skillExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            HStack(alignment: .firstTextBaseline, spacing: tokens.spacing.s) {
                HStack(spacing: tokens.spacing.xs) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                    Text(verbatim: chip.name)
                }
                .font(tokens.mono)
                .padding(.horizontal, tokens.spacing.s)
                .padding(.vertical, tokens.spacing.xxs)
                .background(DesignTokens.userMessageFill, in: Capsule())
                .overlay(Capsule().strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline))
                if !chip.arguments.isEmpty {
                    Text(verbatim: chip.arguments)
                        .font(tokens.mono)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel("Command \(chip.name) \(chip.arguments)")
            if let skillBody = chip.skillBody {
                DisclosureGroup("Skill · \(skillName)", isExpanded: $skillExpanded) {
                    TranscriptBoundedTextView(text: skillBody)
                }
                .font(.caption)
                .onChange(of: revealForSearch, initial: true) { _, reveal in if reveal { skillExpanded = true } }
            }
            if let output = chip.output, !output.isEmpty {
                if revealForSearch {
                    // A find match can sit on any line, and the output can be a large dump.
                    TranscriptBoundedTextView(text: output)
                } else {
                    Text(verbatim: output)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .help(output)
                }
            }
        }
    }

    private var skillName: String {
        chip.name.hasPrefix("/") ? String(chip.name.dropFirst()) : chip.name
    }
}
