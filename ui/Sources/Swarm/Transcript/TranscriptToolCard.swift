import AppKit
import SwiftUI
import SwarmCore
import TranscriptTool

struct TranscriptToolCard: View {
    let activity: TranscriptToolActivity
    var revealForSearch = false
    @State private var expanded = false
    @State private var inputExpanded = false
    @State private var bodyExpanded = false
    @State private var revealingFile = false

    var body: some View {
        let title = activity.headerTitle
        let counts = activity.diffCounts
        return VStack(alignment: .leading, spacing: 0) {
            // One line when closed: status, tool, title, diff size, exit and time. Click or Space opens it.
            Button { expanded.toggle() } label: {
                HStack(spacing: DesignTokens.Spacing.s) {
                    TranscriptStatusGlyph(state: activity.state)
                        .help(TranscriptStatusGlyph.label(activity.state)
                            + ". The status describes the tool result; read the output for verification results.")
                    Text(verbatim: activity.name).fontWeight(.semibold).lineLimit(1).layoutPriority(1)
                    Text(verbatim: title)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: DesignTokens.Spacing.s)
                    if let counts {
                        HStack(spacing: DesignTokens.Spacing.xs) {
                            Text(verbatim: "+\(counts.added)").foregroundStyle(DesignTokens.color(.done))
                            Text(verbatim: "−\(counts.removed)").foregroundStyle(DesignTokens.color(.failed))
                        }
                        .font(DesignTokens.mono)
                    }
                    if !resultLabel.isEmpty {
                        Text(verbatim: resultLabel)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(activity.state == .failed ? DesignTokens.color(.failed) : .secondary)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(accessibilityLabel(title: title, counts: counts))
            .accessibilityValue(expanded ? "Expanded" : "Collapsed")
            .accessibilityHint(expanded ? "Hides the tool details" : "Shows the tool details")
            .accessibilityIdentifier("transcript-tool-card")
            if let skillBody = activity.skillBody {
                DisclosureGroup("Body", isExpanded: $bodyExpanded) {
                    TranscriptBoundedTextView(text: skillBody)
                }
                .font(.caption)
                .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.s)
            }
            if expanded {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                    if let command = activity.command {
                        TranscriptOutputView(text: command, title: "Command")
                    }
                    if let path = activity.path {
                        HStack {
                            Text(verbatim: path).font(DesignTokens.mono)
                                .textSelection(.enabled)
                            Spacer(minLength: DesignTokens.Spacing.s)
                            if path.hasPrefix("/") {
                                Button(revealingFile ? "Opening Finder…" : "Reveal file", systemImage: "folder") {
                                    revealingFile = true
                                    Task {
                                        try? await Task.sleep(for: .milliseconds(30))
                                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                                        revealingFile = false
                                    }
                                }
                                .disabled(revealingFile)
                                .help("Reveal file in Finder")
                            }
                        }
                    }
                    if let output = activity.output {
                        TranscriptOutputView(text: output, title: "Output", revealAll: revealForSearch, lineLimit: 5)
                    } else {
                        Text(activity.state == .waiting ? "Waiting for tool result." : "No tool result was recorded.")
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    ForEach(activity.diffs.indices, id: \.self) { index in
                        TranscriptDiffView(diff: activity.diffs[index], revealForSearch: revealForSearch)
                    }
                    DisclosureGroup("Tool input", isExpanded: $inputExpanded) {
                        TranscriptToolInputView(input: activity.input)
                    }
                    .font(.caption)
                }
                .padding(.top, DesignTokens.Spacing.s)
                .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.s)
            }
        }
        .padding(.horizontal, DesignTokens.Spacing.s)
        .padding(.vertical, DesignTokens.Spacing.xs)
        .background(expanded ? DesignTokens.userMessageFill : .clear, in: .rect(cornerRadius: DesignTokens.Radius.control))
        // The fill bleeds past the column so the glyph and the right label share the edges of the other rows.
        .padding(.horizontal, -DesignTokens.Spacing.s)
        .buttonStyle(.borderless)
        .onAppear {
            if activity.state == .failed || revealForSearch { expanded = true }
            if revealForSearch { inputExpanded = true; bodyExpanded = true }
        }
        .onChange(of: activity.state) { _, state in if state == .failed { expanded = true } }
        .onChange(of: revealForSearch) { _, reveal in
            if reveal { expanded = true; inputExpanded = true; bodyExpanded = true }
        }
    }

    /// "exit 1 · 8.6s", "exit 0", "0.4s", or "".
    private var resultLabel: String {
        resultParts.joined(separator: " · ")
    }

    private var resultParts: [String] {
        [activity.exitCode.map { "exit \($0)" }, activity.duration.map(TranscriptToolActivity.durationLabel)]
            .compactMap { $0 }
    }

    /// "Edit TranscriptView.swift, 12 added, 3 removed, exit 1, 8.6s, Failed": what the header shows.
    private func accessibilityLabel(title: String, counts: TranscriptToolActivity.DiffCounts?) -> String {
        let countParts = counts.map { ["\($0.added) added", "\($0.removed) removed"] } ?? []
        return (["\(activity.name) \(title)"] + countParts + resultParts
            + [TranscriptStatusGlyph.label(activity.state)]).joined(separator: ", ")
    }
}

/// A tool's or a shell command's result in the row's glyph slot.
struct TranscriptStatusGlyph: View {
    let state: TranscriptToolActivity.State

    var body: some View {
        Group {
            switch state {
            case .waiting: Text(verbatim: "●").foregroundStyle(DesignTokens.color(.working))
            case .finished: Text(verbatim: "✓").foregroundStyle(DesignTokens.color(.done))
            case .failed: Text(verbatim: "×").foregroundStyle(DesignTokens.color(.failed))
            case .interrupted: Image(systemName: "stop.circle").foregroundStyle(.orange)
            case .unreported: Image(systemName: "questionmark.circle").foregroundStyle(.secondary)
            }
        }
        .frame(width: DesignTokens.Size.glyphSlot)
        .accessibilityLabel(Self.label(state))
    }

    static func label(_ state: TranscriptToolActivity.State) -> String {
        switch state {
        case .waiting: "Waiting for result"
        case .finished: "Finished"
        case .failed: "Failed"
        case .interrupted: "Interrupted"
        case .unreported: "No result"
        }
    }
}

struct TranscriptOutputView: View {
    let text: String
    let title: String
    var revealAll = false
    var lineLimit = 120
    @State private var showAll = false
    @State private var preview: TranscriptTextPreview?
    @State private var copying = false

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack {
                Text(title).font(.caption.weight(.medium)).foregroundStyle(.secondary)
                Spacer()
                Button(copying ? "Copying…" : "Copy \(title.lowercased())", systemImage: "doc.on.doc") {
                    copying = true
                    Task {
                        try? await Task.sleep(for: .milliseconds(30))
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(text, forType: .string)
                        copying = false
                    }
                }
                .disabled(copying)
                .font(.caption).buttonStyle(.borderless)
            }
            Group {
                if showAll || revealAll {
                    TranscriptBoundedTextView(text: text, emptyText: "No output was recorded.")
                } else if let preview {
                    TranscriptBoundedTextView(text: preview.text, emptyText: "No output was recorded.")
                } else {
                    DelayedProgress("Preparing preview…")
                        .frame(height: DesignTokens.Size.collapsedOutput, alignment: .leading)
                }
            }
            .background(DesignTokens.codeBlockFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
            if let preview, preview.isTruncated, !revealAll {
                TranscriptShowAllButton(showAll: $showAll, hiddenLineCount: preview.hiddenLineCount)
            }
        }
        .task(id: text) {
            preview = nil
            let source = text
            let limit = lineLimit
            let prepared = await Task.detached(priority: .userInitiated) {
                TranscriptTextPreview(source, lineLimit: limit)
            }.value
            guard !Task.isCancelled else { return }
            preview = prepared
        }
    }
}

/// Opens or closes folded output: "… +N lines · Show all", "… Show all", or "Show less".
struct TranscriptShowAllButton: View {
    @Binding var showAll: Bool
    let hiddenLineCount: Int

    var body: some View {
        let title: LocalizedStringKey = showAll ? "Show less"
            : hiddenLineCount > 0 ? "… +^[\(hiddenLineCount) line](inflect: true) · Show all" : "… Show all"
        Button(title) { showAll.toggle() }
            .font(.caption).foregroundStyle(.secondary).buttonStyle(.borderless)
    }
}

private struct TranscriptToolInputView: View {
    let input: JSONElement
    @State private var encoded: String?

    var body: some View {
        Group {
            if let encoded {
                TranscriptOutputView(text: encoded, title: "Input")
            } else {
                DelayedProgress("Preparing input…")
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: input) {
            encoded = nil
            let value = input
            let prepared = await Task.detached(priority: .userInitiated) {
                value.compactJSON
            }.value
            guard !Task.isCancelled else { return }
            encoded = prepared
        }
    }
}

struct TranscriptDiffView: View {
    let diff: TranscriptDiff
    var revealForSearch = false
    @State private var expanded = false
    @State private var split = false
    @State private var showFull = false
    @State private var preparedDiff: TranscriptDiff?
    @State private var limited: TranscriptDiffPreview?
    @State private var full: TranscriptDiffPreview?
    @State private var copying = false
    @State private var counts: (added: Int, removed: Int)?
    @State private var rendered = false
    @State private var renderError = false

    private struct PreviewRequest: Equatable {
        let diff: TranscriptDiff
        let expanded: Bool
        let full: Bool
    }

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            if expanded {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                    Text(verbatim: diff.path).font(.caption).textSelection(.enabled)
                    HStack {
                        Picker("Diff layout", selection: $split) {
                            Text("Unified").tag(false)
                            Text("Split").tag(true)
                        }.pickerStyle(.segmented).frame(width: DesignTokens.Size.segmentedPicker)
                        Spacer()
                        Button(copying ? "Preparing patch…" : "Copy patch", systemImage: "doc.on.doc") {
                            copying = true
                            Task {
                                try? await Task.sleep(for: .milliseconds(30))
                                let patch: String
                                if let full { patch = full.patch }
                                else {
                                    let source = diff
                                    let prepared = await Task.detached(priority: .userInitiated) {
                                        TranscriptDiffPreview(source, full: true)
                                    }.value
                                    if source == diff { full = prepared }
                                    patch = prepared.patch
                                }
                                NSPasteboard.general.clearContents()
                                NSPasteboard.general.setString(patch, forType: .string)
                                copying = false
                            }
                        }
                        .disabled(copying)
                        .font(.caption).buttonStyle(.borderless)
                    }
                    if let preview = showFull || revealForSearch ? full : limited {
                        if let notice = preview.notice {
                            Text(verbatim: notice).font(.caption).foregroundStyle(.secondary)
                        }
                        ZStack {
                            DiffWebView(text: preview.patch, isDiff: true, split: split) { success in
                                rendered = true
                                renderError = !success
                            }
                            if !rendered { DelayedProgress("Rendering patch…") }
                            if renderError { Text("The patch view could not load.").foregroundStyle(.red) }
                        }
                        .frame(height: DesignTokens.Size.outputPreview)
                        .onChange(of: split) { _, _ in rendered = false; renderError = false }
                        .onChange(of: preview.patch) { _, _ in rendered = false; renderError = false }
                    } else {
                        DelayedProgress("Preparing patch…")
                            .frame(height: DesignTokens.Size.collapsedOutput, alignment: .leading)
                    }
                    if limited?.notice != nil, !revealForSearch {
                        Button(showFull ? "Show preview" : "Show full patch") { showFull.toggle() }
                            .font(.caption).buttonStyle(.borderless)
                    }
                }
                .padding(.top, DesignTokens.Spacing.s)
            }
        } label: {
            Label("\((diff.path as NSString).lastPathComponent) · +\(counts?.added.description ?? "…") −\(counts?.removed.description ?? "…")", systemImage: "doc.text")
                .font(.callout).help(diff.path)
        }
        .onAppear { if revealForSearch { expanded = true } }
        .onChange(of: revealForSearch) { _, reveal in if reveal { expanded = true } }
        .task(id: PreviewRequest(diff: diff, expanded: expanded, full: showFull || revealForSearch)) {
            if preparedDiff != diff {
                preparedDiff = diff
                limited = nil
                full = nil
                rendered = false
            }
            guard expanded else { return }
            let needsFull = showFull || revealForSearch
            if needsFull ? full != nil : limited != nil { return }
            do { try await Task.sleep(for: .milliseconds(50)) }
            catch { return }
            let source = diff
            let prepared = await Task.detached(priority: .userInitiated) {
                TranscriptDiffPreview(source, full: needsFull)
            }.value
            guard !Task.isCancelled else { return }
            if needsFull { full = prepared } else { limited = prepared }
        }
        .task(id: diff) {
            let source = diff
            let result = await Task.detached(priority: .utility) {
                var added = 0
                var removed = 0
                for hunk in source.hunks {
                    for line in hunk.lines {
                        if line.hasPrefix("+") { added += 1 }
                        else if line.hasPrefix("-") { removed += 1 }
                    }
                }
                return (added, removed)
            }.value
            guard !Task.isCancelled else { return }
            counts = result
        }
    }
}
