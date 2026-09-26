import AppKit
import SwiftUI
import SwarmCore

/// Renders a transcript message as structured native SwiftUI blocks.
struct TranscriptMessageView: View {
    let text: String

    var body: some View {
        let blocks = TranscriptMessageBlocks.parse(text)
        if blocks.isEmpty {
            EmptyView()
        } else {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(blocks) { block in
                    TranscriptBlockView(block: block)
                }
            }
            .textSelection(.enabled)
            .environment(\.openURL, OpenURLAction { url in
                guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
                    return .handled
                }
                return .systemAction
            })
        }
    }
}

private struct TranscriptBlockView: View {
    let block: TranscriptMessageBlock

    var body: some View {
        switch block {
        case let .paragraph(_, text):
            Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                .font(.body)
                .lineSpacing(3)
                .frame(maxWidth: .infinity, alignment: .leading)

        case let .heading(_, level, text):
            Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                .font(headingFont(level))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, level <= 2 ? 6 : 2)

        case let .codeBlock(_, language, code):
            CodeBlockView(language: language, code: code)

        case let .blockquote(_, text):
            HStack(alignment: .top, spacing: 10) {
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(.secondary.opacity(0.5))
                    .frame(width: 3)
                Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .lineSpacing(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, 2)

        case let .unorderedList(_, items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("•")
                            .font(.body.weight(.bold))
                            .foregroundStyle(.secondary)
                        Text(TranscriptMessageBlocks.parseInlineMarkdown(item))
                            .font(.body)
                            .lineSpacing(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }

        case let .orderedList(_, startIndex, items):
            VStack(alignment: .leading, spacing: 5) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    let itemNumber = startIndex + index
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("\(itemNumber).")
                            .font(.callout.monospacedDigit())
                            .foregroundStyle(.secondary)
                        Text(TranscriptMessageBlocks.parseInlineMarkdown(item))
                            .font(.body)
                            .lineSpacing(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }

        case let .table(_, headers, rows, rawText):
            TableBlockView(headers: headers, rows: rows, rawText: rawText)

        case let .rawMonospace(_, text):
            RawMonospaceBlockView(text: text)

        case .divider:
            Divider()
                .padding(.vertical, 4)
        }
    }

    private func headingFont(_ level: Int) -> Font {
        switch level {
        case 1: .title2.bold()
        case 2: .title3.bold()
        case 3: .headline.bold()
        default: .subheadline.bold()
        }
    }
}

private struct CodeBlockView: View {
    let language: String?
    let code: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language?.lowercased() ?? "code")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(code, forType: .string)
                    copied = true
                    Task {
                        try? await Task.sleep(nanoseconds: 1_500_000_000)
                        copied = false
                    }
                } label: {
                    Label(copied ? "Copied" : "Copy code", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .labelStyle(.iconOnly)
                }
                .buttonStyle(.borderless)
                .font(.caption)
                .accessibilityLabel(accessibilityLabelText)
                .help("Copy code")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.quaternary.opacity(0.3))

            ScrollView(.horizontal, showsIndicators: true) {
                Text(verbatim: code)
                    .font(.system(.callout, design: .monospaced))
                    .fixedSize(horizontal: true, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
            }
        }
        .background(.background.opacity(0.5), in: RoundedRectangle(cornerRadius: 6))
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .strokeBorder(.quaternary, lineWidth: 1)
        )
    }

    private var accessibilityLabelText: String {
        if let language, !language.isEmpty {
            return "Copy \(language) code"
        }
        return "Copy code"
    }
}

private struct TableBlockView: View {
    let headers: [String]
    let rows: [[String]]
    let rawText: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Table")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(rawText, forType: .string)
                    copied = true
                    Task {
                        try? await Task.sleep(nanoseconds: 1_500_000_000)
                        copied = false
                    }
                } label: {
                    Label(copied ? "Copied" : "Copy table", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .labelStyle(.iconOnly)
                }
                .buttonStyle(.borderless)
                .font(.caption)
                .accessibilityLabel("Copy table")
                .help("Copy table")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.quaternary.opacity(0.3))

            ScrollView(.horizontal, showsIndicators: true) {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 8) {
                    if !headers.isEmpty {
                        GridRow {
                            ForEach(Array(headers.enumerated()), id: \.offset) { _, header in
                                Text(TranscriptMessageBlocks.parseInlineMarkdown(header))
                                    .font(.body.weight(.semibold))
                            }
                        }
                        Divider()
                    }
                    ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                        GridRow {
                            ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                                Text(TranscriptMessageBlocks.parseInlineMarkdown(cell))
                                    .font(.body)
                            }
                        }
                    }
                }
                .padding(10)
            }
        }
        .background(.background.opacity(0.5), in: RoundedRectangle(cornerRadius: 6))
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .strokeBorder(.quaternary, lineWidth: 1)
        )
    }
}

private struct RawMonospaceBlockView: View {
    let text: String

    var body: some View {
        ScrollView(.horizontal, showsIndicators: true) {
            Text(verbatim: text)
                .font(.system(.callout, design: .monospaced))
                .fixedSize(horizontal: true, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
        }
        .background(.background.opacity(0.5), in: RoundedRectangle(cornerRadius: 6))
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .strokeBorder(.quaternary, lineWidth: 1)
        )
    }
}
