import SwiftUI
import SwarmCore

/// A `!` command the owner ran in Claude Code's shell mode: `$ command` on one line, and its output
/// under it in a mono box folded at 50 lines.
struct TranscriptShellRow: View {
    @Environment(\.designTokens) private var tokens
    let run: TranscriptShellRun
    var revealForSearch = false
    @State private var showAll = false

    private static let foldLineLimit = 50

    var body: some View {
        // Claude Code moves long shell output into <persisted-output>, so the fold is built here,
        // not in a task, and the row keeps its height once it appears. Past the 12,000-character
        // preview, the only cost is one byte scan of the output for its line count.
        let preview = TranscriptTextPreview(run.output, lineLimit: Self.foldLineLimit)
        let expanded = showAll || revealForSearch
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            if let command = run.command {
                HStack(spacing: tokens.spacing.s) {
                    if let exitCode = run.exitCode {
                        TranscriptStatusGlyph(state: exitCode == 0 ? .finished : .failed)
                    } else {
                        Text(verbatim: "$")
                            .font(tokens.mono)
                            .foregroundStyle(.secondary)
                            .frame(width: DesignTokens.Size.glyphSlot)
                    }
                    Text(verbatim: command)
                        .font(tokens.mono)
                        .textSelection(.enabled)
                    Spacer(minLength: tokens.spacing.s)
                    Text(verbatim: note(lineCount: preview.lineCount))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(noteColor)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Shell command \(command), \(note(lineCount: preview.lineCount))")
            }
            if !run.output.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    TranscriptBoundedTextView(text: expanded ? run.output : preview.text)
                    if preview.isTruncated, !revealForSearch {
                        TranscriptShowAllButton(showAll: $showAll, hiddenLineCount: preview.hiddenLineCount)
                            .padding([.horizontal, .bottom], tokens.spacing.s)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(DesignTokens.codeBlockFill, in: .rect(cornerRadius: DesignTokens.Radius.control))
                .overlay(
                    RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                        .strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline)
                )
                .padding(.leading, DesignTokens.Size.glyphSlot + tokens.spacing.s)
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Shell output")
            }
        }
    }

    /// "exit N" when the log has an exit code, else "no output" or the output's line count.
    private func note(lineCount: Int) -> String {
        if let exitCode = run.exitCode { return "exit \(exitCode)" }
        if run.output.isEmpty { return "no output" }
        return String(AttributedString(localized: "^[\(lineCount) line](inflect: true)").characters)
    }

    private var noteColor: Color {
        switch run.exitCode {
        case nil: .secondary
        case 0: DesignTokens.color(.done)
        default: DesignTokens.color(.failed)
        }
    }
}
