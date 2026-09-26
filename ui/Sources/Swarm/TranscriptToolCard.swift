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

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            DisclosureGroup(isExpanded: $expanded) {
                VStack(alignment: .leading, spacing: 12) {
                    if let command = activity.command {
                        TranscriptOutputView(text: command, title: "Command")
                    }
                    if let path = activity.path {
                        HStack {
                            Text(verbatim: path).font(.caption.monospaced())
                                .textSelection(.enabled)
                            Spacer(minLength: 8)
                            if path.hasPrefix("/") {
                                Button("Reveal file", systemImage: "folder") {
                                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
                                }
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
                    ForEach(Array(activity.diffs.enumerated()), id: \.offset) { _, diff in
                        TranscriptDiffView(diff: diff, revealForSearch: revealForSearch)
                    }
                    DisclosureGroup("Tool input", isExpanded: $inputExpanded) {
                        TranscriptOutputView(text: activity.input.compactJSON, title: "Input")
                    }
                    .font(.caption)
                }
                .padding(.top, 10)
            } label: {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: activity.command == nil ? "wrench.and.screwdriver" : "terminal")
                        .foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(verbatim: title).font(.callout.weight(.medium)).lineLimit(2)
                        if let command = activity.command?.components(separatedBy: .newlines).first,
                           !command.isEmpty, !title.contains(command) {
                            Text(verbatim: command).font(.caption.monospaced())
                                .foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    Spacer(minLength: 8)
                    Label(stateLabel, systemImage: stateSymbol)
                        .font(.caption).foregroundStyle(stateColor)
                        .fixedSize()
                        .help("The status describes the tool result. Read the output for verification results.")
                }
                .padding(.vertical, 2)
            }
            .accessibilityIdentifier("transcript-tool-card")
        }
        .padding(12)
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.quaternary))
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

    var body: some View {
        let preview = TranscriptTextPreview(text)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title).font(.caption.weight(.medium)).foregroundStyle(.secondary)
                Spacer()
                Button("Copy \(title.lowercased())", systemImage: "doc.on.doc") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                }
                .font(.caption).buttonStyle(.borderless)
            }
            ScrollView(.horizontal) {
                Text(verbatim: text.isEmpty ? "No output was recorded." : (showAll || revealAll ? text : preview.text))
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: true, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
            }
            .background(.background.opacity(0.5), in: RoundedRectangle(cornerRadius: 6))
            if preview.isTruncated {
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
    }
}

struct TranscriptDiffView: View {
    let diff: TranscriptDiff
    var revealForSearch = false
    @State private var expanded = false
    @State private var split = false
    @State private var showFull = false

    var body: some View {
        let added = diff.hunks.reduce(0) { $0 + $1.lines.filter { $0.hasPrefix("+") }.count }
        let removed = diff.hunks.reduce(0) { $0 + $1.lines.filter { $0.hasPrefix("-") }.count }
        DisclosureGroup(isExpanded: $expanded) {
            if expanded {
                let limited = TranscriptDiffPreview(diff)
                let preview = showFull || revealForSearch ? TranscriptDiffPreview(diff, full: true) : limited
                VStack(alignment: .leading, spacing: 8) {
                    Text(verbatim: diff.path).font(.caption).textSelection(.enabled)
                    HStack {
                        Picker("Diff layout", selection: $split) {
                            Text("Unified").tag(false)
                            Text("Split").tag(true)
                        }.pickerStyle(.segmented).frame(width: 145)
                        Spacer()
                        Button("Copy patch", systemImage: "doc.on.doc") {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(TranscriptDiffPreview(diff, full: true).patch, forType: .string)
                        }.font(.caption).buttonStyle(.borderless)
                    }
                    if let notice = preview.notice {
                        Text(verbatim: notice).font(.caption).foregroundStyle(.secondary)
                    }
                    if limited.notice != nil, !revealForSearch {
                        Button(showFull ? "Show preview" : "Show full patch") { showFull.toggle() }
                            .font(.caption).buttonStyle(.borderless)
                    }
                    DiffWebView(text: preview.patch, isDiff: true, split: split)
                        .frame(height: 300)
                }
                .padding(.top, 8)
            }
        } label: {
            Label("\((diff.path as NSString).lastPathComponent) · +\(added) −\(removed)", systemImage: "doc.text")
                .font(.callout).help(diff.path)
        }
        .onAppear { if revealForSearch { expanded = true } }
        .onChange(of: revealForSearch) { _, reveal in if reveal { expanded = true } }
    }
}
