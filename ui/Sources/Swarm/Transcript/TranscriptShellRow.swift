import SwiftUI
import SwarmCore

/// A `!` command the owner ran in Claude Code's shell mode: `$ command` on one line, and its output
/// under it in a mono box folded at 50 lines.
struct TranscriptShellRow: View {
    let run: TranscriptShellRun
    var revealForSearch = false
    @State private var showAll = false

    var body: some View {
        // Claude Code moves long shell output into <persisted-output>, so the fold is built here,
        // not in a task, and the row keeps its height once it appears.
        let preview = TranscriptTextPreview(run.output, lineLimit: 50)
        let lineCount = run.output.split(separator: "\n", omittingEmptySubsequences: false).count
        let expanded = showAll || revealForSearch
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            if let command = run.command {
                HStack(spacing: DesignTokens.Spacing.s) {
                    if let exitCode = run.exitCode {
                        TranscriptStatusGlyph(state: exitCode == 0 ? .finished : .failed)
                    } else {
                        Text(verbatim: "$")
                            .font(DesignTokens.mono)
                            .foregroundStyle(.secondary)
                            .frame(width: DesignTokens.Size.glyphSlot)
                    }
                    Text(verbatim: command)
                        .font(DesignTokens.mono)
                        .textSelection(.enabled)
                    Spacer(minLength: DesignTokens.Spacing.s)
                    Text(verbatim: note(lineCount: lineCount))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(noteColor)
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Shell command \(command), \(note(lineCount: lineCount))")
            }
            if !run.output.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    TranscriptBoundedTextView(text: expanded ? run.output : preview.text)
                    if preview.isTruncated, !revealForSearch {
                        let hidden = lineCount - min(lineCount, 50)
                        let title: LocalizedStringKey = showAll ? "Show less"
                            : hidden > 0 ? "… +^[\(hidden) line](inflect: true) · Show all" : "… Show all"
                        Button(title) {
                            showAll.toggle()
                        }
                        .buttonStyle(.borderless)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .padding([.horizontal, .bottom], DesignTokens.Spacing.s)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(DesignTokens.codeBlockFill, in: .rect(cornerRadius: DesignTokens.Radius.control))
                .overlay(
                    RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                        .strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline)
                )
                .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.s)
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Shell output")
            }
        }
    }

    /// "exit N" when the log has an exit code, else "no output" or the output's line count.
    private func note(lineCount: Int) -> String {
        if let exitCode = run.exitCode { return "exit \(exitCode)" }
        if run.output.isEmpty { return "no output" }
        return lineCount == 1 ? "1 line" : "\(lineCount) lines"
    }

    private var noteColor: Color {
        switch run.exitCode {
        case nil: .secondary
        case 0: DesignTokens.color(.done)
        default: DesignTokens.color(.failed)
        }
    }
}
