import AppKit
import SwiftUI
import SwarmCore

/// Renders a transcript message as structured native SwiftUI blocks.
struct TranscriptMessageView: View {
    let text: String
    @State private var blocks: [TranscriptMessageBlock]?

    var body: some View {
        let small = text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) == nil
        let displayed = small ? TranscriptMessageBlocks.parse(text) : blocks
        return Group {
            if let blocks = displayed, !blocks.isEmpty {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
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
            } else if displayed == nil {
                ProgressView("Preparing message…")
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: text) {
            guard !small else { return }
            let source = text
            let prepared = await Task.detached(priority: .userInitiated) {
                TranscriptMessageBlocks.parse(source)
            }.value
            guard !Task.isCancelled else { return }
            blocks = prepared
        }
    }
}

private struct TranscriptBlockView: View {
    let block: TranscriptMessageBlock

    var body: some View {
        switch block {
        case let .paragraph(_, text):
            Group {
                if text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) != nil {
                    TranscriptBoundedTextView(text: text)
                } else {
                    Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                        .font(.body)
                        .lineSpacing(3)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

        case let .heading(_, level, text):
            Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                .font(headingFont(level))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, level <= 2 ? 6 : 2)

        case let .codeBlock(_, language, code):
            CodeBlockView(language: language, code: code)

        case let .blockquote(_, text):
            HStack(alignment: .top, spacing: DesignTokens.Spacing.s) {
                RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                    .fill(DesignTokens.quoteBar)
                    .frame(width: DesignTokens.Size.quoteBar)
                if text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) != nil {
                    TranscriptBoundedTextView(text: text)
                        .foregroundStyle(.secondary)
                } else {
                    Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .lineSpacing(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.vertical, DesignTokens.Spacing.xxs)

        case let .unorderedList(_, items):
            LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                ForEach(items.indices, id: \.self) { index in
                    HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
                        Text("•")
                            .font(.body.weight(.bold))
                            .foregroundStyle(.secondary)
                        TranscriptListItemText(text: items[index])
                    }
                }
            }

        case let .orderedList(_, startIndex, items):
            LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                ForEach(items.indices, id: \.self) { index in
                    let itemNumber = startIndex + index
                    HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
                        Text("\(itemNumber).")
                            .font(.callout.monospacedDigit())
                            .foregroundStyle(.secondary)
                        TranscriptListItemText(text: items[index])
                    }
                }
            }

        case let .table(_, headers, rows, rawText):
            TableBlockView(headers: headers, rows: rows, rawText: rawText)

        case let .rawMonospace(_, text):
            RawMonospaceBlockView(text: text)

        case .divider:
            Divider()
                .padding(.vertical, DesignTokens.Spacing.xs)
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

private struct TranscriptListItemText: View {
    let text: String

    var body: some View {
        if text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) != nil {
            TranscriptBoundedTextView(text: text)
        } else {
            Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                .font(.body)
                .lineSpacing(2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct CodeBlockView: View {
    let language: String?
    let code: String
    @State private var copied = false
    @State private var copying = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language?.lowercased() ?? "code")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    copying = true
                    Task {
                        try? await Task.sleep(for: .milliseconds(30))
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(code, forType: .string)
                        copying = false
                        copied = true
                        try? await Task.sleep(nanoseconds: 1_500_000_000)
                        copied = false
                    }
                } label: {
                    Label(copying ? "Copying…" : copied ? "Copied" : "Copy code", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .labelStyle(.iconOnly)
                }
                .disabled(copying)
                .buttonStyle(.borderless)
                .font(.caption)
                .accessibilityLabel(accessibilityLabelText)
                .help("Copy code")
            }
            .padding(.horizontal, DesignTokens.Spacing.s)
            .padding(.vertical, DesignTokens.Spacing.s)
            .background(DesignTokens.codeHeaderFill)

            TranscriptBoundedTextView(text: code)
        }
        .background(DesignTokens.codeBlockFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
        .overlay(
            RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                .strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline)
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
    @State private var copying = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Table")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    copying = true
                    Task {
                        try? await Task.sleep(for: .milliseconds(30))
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(rawText, forType: .string)
                        copying = false
                        copied = true
                        try? await Task.sleep(nanoseconds: 1_500_000_000)
                        copied = false
                    }
                } label: {
                    Label(copying ? "Copying…" : copied ? "Copied" : "Copy table", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .labelStyle(.iconOnly)
                }
                .disabled(copying)
                .buttonStyle(.borderless)
                .font(.caption)
                .accessibilityLabel("Copy table")
                .help("Copy table")
            }
            .padding(.horizontal, DesignTokens.Spacing.s)
            .padding(.vertical, DesignTokens.Spacing.s)
            .background(DesignTokens.codeHeaderFill)

            if rows.count > 30 || headers.count > 20 || rows.contains(where: { $0.count > 20 }) {
                TranscriptBoundedTextView(text: rawText)
            } else {
                ScrollView(.horizontal, showsIndicators: true) {
                    Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 8) {
                        if !headers.isEmpty {
                            GridRow {
                                ForEach(headers.indices, id: \.self) { index in
                                    TranscriptTableCell(text: headers[index], isHeader: true)
                                }
                            }
                            Divider()
                        }
                        ForEach(rows.indices, id: \.self) { rowIndex in
                            GridRow {
                                ForEach(rows[rowIndex].indices, id: \.self) { cellIndex in
                                    TranscriptTableCell(text: rows[rowIndex][cellIndex])
                                }
                            }
                        }
                    }
                    .padding(DesignTokens.Spacing.s)
                }
            }
        }
        .background(DesignTokens.codeBlockFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
        .overlay(
            RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                .strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline)
        )
    }
}

private struct TranscriptTableCell: View {
    let text: String
    var isHeader = false

    var body: some View {
        if text.index(text.startIndex, offsetBy: 4_096, limitedBy: text.endIndex) != nil {
            TranscriptBoundedTextView(text: text)
        } else {
            Text(TranscriptMessageBlocks.parseInlineMarkdown(text))
                .font(isHeader ? .body.weight(.semibold) : .body)
        }
    }
}

private struct RawMonospaceBlockView: View {
    let text: String

    var body: some View {
        TranscriptBoundedTextView(text: text)
        .background(DesignTokens.codeBlockFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
        .overlay(
            RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                .strokeBorder(.quaternary, lineWidth: DesignTokens.Size.hairline)
        )
    }
}
