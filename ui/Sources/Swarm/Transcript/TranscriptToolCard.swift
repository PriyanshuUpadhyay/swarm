import AppKit
import SwiftUI
import SwarmCore
import TranscriptTool

struct TranscriptToolCard: View {
    let title: String
    let activity: TranscriptToolActivity
    var revealForSearch = false
    @State private var expanded = false
    @State private var inputExpanded = false
    @State private var revealingFile = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // One line when closed: glyph, tool, target, duration, result. Click or Space opens it.
            Button { expanded.toggle() } label: {
                HStack(spacing: DesignTokens.Spacing.s) {
                    Image(systemName: expanded ? "chevron.down" : "chevron.right")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .frame(width: DesignTokens.Size.glyphSlot)
                    Image(systemName: activity.command == nil ? "wrench.and.screwdriver" : "terminal")
                        .foregroundStyle(.secondary)
                    Text(verbatim: activity.name).fontWeight(.medium).lineLimit(1).layoutPriority(1)
                    if let target {
                        Text(verbatim: target)
                            .font(DesignTokens.mono)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer(minLength: DesignTokens.Spacing.s)
                    if let duration = activity.duration {
                        Text(TranscriptToolActivity.durationLabel(duration))
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                    }
                    Image(systemName: stateSymbol)
                        .foregroundStyle(stateColor)
                        .help(stateLabel + ". The status describes the tool result; read the output for verification results.")
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(activity.name) \(target ?? ""), \(stateLabel)")
            .accessibilityHint(expanded ? "Hides the tool details" : "Shows the tool details")
            .accessibilityIdentifier("transcript-tool-card")
            if expanded {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                    Text(verbatim: title).font(.callout).foregroundStyle(.secondary)
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
                        TranscriptOutputView(text: output, title: "Output", revealAll: revealForSearch)
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
        .buttonStyle(.borderless)
        .onAppear {
            if activity.state == .failed || revealForSearch { expanded = true }
            if revealForSearch { inputExpanded = true }
        }
        .onChange(of: activity.state) { _, state in if state == .failed { expanded = true } }
        .onChange(of: revealForSearch) { _, reveal in
            if reveal { expanded = true; inputExpanded = true }
        }
    }

    /// The file name, or the command's first line.
    private var target: String? {
        if let path = activity.path { return (path as NSString).lastPathComponent }
        return activity.command.map { String($0.prefix(200).prefix(while: { !$0.isNewline })) }
            .flatMap { $0.isEmpty ? nil : $0 }
    }

    private var stateLabel: String {
        switch activity.state {
        case .waiting: "Waiting for result"
        case .finished: "Finished"
        case .failed: "Failed"
        case .interrupted: "Interrupted"
        case .unreported: "No result"
        }
    }

    private var stateSymbol: String {
        switch activity.state {
        case .waiting: "clock"
        case .finished: "checkmark.circle"
        case .failed: "exclamationmark.circle"
        case .interrupted: "stop.circle"
        case .unreported: "questionmark.circle"
        }
    }

    private var stateColor: Color {
        switch activity.state {
        case .failed: .red
        case .interrupted: .orange
        default: .secondary
        }
    }
}

struct TranscriptOutputView: View {
    let text: String
    let title: String
    var revealAll = false
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
            if preview?.isTruncated == true {
                HStack {
                    Text(showAll || revealAll ? "Full output shown." : "Preview only. Some output is hidden.")
                        .font(.caption).foregroundStyle(.secondary)
                    if !revealAll {
                        Button(showAll ? "Show less" : "Show full output") { showAll.toggle() }
                            .font(.caption).buttonStyle(.borderless)
                    }
                }
            }
        }
        .task(id: text) {
            preview = nil
            let source = text
            let prepared = await Task.detached(priority: .userInitiated) {
                TranscriptTextPreview(source)
            }.value
            guard !Task.isCancelled else { return }
            preview = prepared
        }
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
