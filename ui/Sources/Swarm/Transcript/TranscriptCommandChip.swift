import SwiftUI
import SwarmCore

/// A slash command the owner typed: a mono capsule with its name, then its arguments, a closed
/// skill body, and the command's local output. It is drawn inside the user's bubble.
struct TranscriptCommandChipView: View {
    let chip: TranscriptCommandChip
    @State private var skillExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
                HStack(spacing: DesignTokens.Spacing.xs) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                    Text(verbatim: chip.name)
                }
                .font(DesignTokens.mono)
                .padding(.horizontal, DesignTokens.Spacing.s)
                .padding(.vertical, DesignTokens.Spacing.xxs)
                .background(DesignTokens.userMessageFill, in: Capsule())
                .overlay(Capsule().strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline))
                if !chip.arguments.isEmpty {
                    Text(verbatim: chip.arguments)
                        .font(DesignTokens.mono)
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
            }
            if let output = chip.output, !output.isEmpty {
                Text(verbatim: output)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .help(output)
            }
        }
    }

    private var skillName: String {
        chip.name.hasPrefix("/") ? String(chip.name.dropFirst()) : chip.name
    }
}
