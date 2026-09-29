import SwiftUI
import SwarmCore

private enum WorkspaceDetailsTab: String {
    case changes = "Changes", branch = "Branch", pullRequest = "PR", usage = "Usage"
}

struct WorkspaceDetails: View {
    let directory: String
    let usage: ChatUsage?
    let hasChat: Bool
    let mode: WorkspaceSidebarMode
    var isActive = true
    let open: (WorkspaceDocument) -> Void
    @State private var branchSelected = false

    private var tab: WorkspaceDetailsTab {
        switch mode {
        case .usage: .usage
        case .pullRequest: .pullRequest
        default: branchSelected ? .branch : .changes
        }
    }

    @State private var workspace: GitWorkspaceSnapshot?
    @State private var workspaceDirectory: String?
    @State private var workspaceRevision = 0
    @State private var comparison: GitBranchComparison?
    @State private var comparisonRevision = -1
    @State private var pullRequest: PullRequestLookup?
    @State private var pullRequestRevision = -1
    @State private var baseRef = ""
    @State private var refreshID = 0
    @State private var loadingWorkspace = true
    @State private var loadingDetail = false
    @State private var workspaceError: String?
    @State private var workspaceErrorDirectory: String?
    @State private var detailError: String?
    @State private var detailErrorDirectory: String?
    @State private var detailErrorTab: WorkspaceDetailsTab?
    @State private var detailErrorBaseRef: String?

    private struct WorkspaceRequest: Equatable {
        let directory: String
        let refresh: Int
        let isActive: Bool
        let needsWorkspace: Bool
    }

    private struct DetailRequest: Equatable {
        let directory: String
        let tab: WorkspaceDetailsTab
        let baseRef: String
        let revision: Int
        let isActive: Bool
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(mode.rawValue).font(.headline)
                Spacer()
                if tab != .usage {
                    Button { loadingWorkspace = true; refreshID += 1 } label: { Image(systemName: "arrow.clockwise") }
                        .help("Refresh this view")
                        .accessibilityLabel("Refresh workspace details")
                        .disabled(loadingWorkspace || loadingDetail || !isActive)
                }
            }
            .buttonStyle(.plain)
            .padding(DesignTokens.Spacing.m)
            if mode == .changes {
                Picker("Changes view", selection: $branchSelected) {
                    Text("Local").tag(false)
                    Text("Branch").tag(true)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .padding(.horizontal, DesignTokens.Spacing.m)
                .padding(.bottom, DesignTokens.Spacing.s)
            }
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                    if tab == .usage {
                        UsageDetails(usage: usage, hasChat: hasChat)
                    } else {
                        if loadingWorkspace || loadingDetail ||
                           (workspaceDirectory != directory && workspaceErrorDirectory != directory) {
                            ProgressView("Reading \(tab.rawValue)…").controlSize(.small)
                        }
                        if let workspaceError, workspaceErrorDirectory == directory {
                            Text(verbatim: workspaceError).foregroundStyle(.red).textSelection(.enabled)
                        }
                        if let detailError, detailErrorDirectory == directory, detailErrorTab == tab,
                           tab != .branch || detailErrorBaseRef == baseRef {
                            Text(verbatim: detailError).foregroundStyle(.red).textSelection(.enabled)
                        }
                        if let workspace, workspaceDirectory == directory {
                            if (workspaceError != nil && workspaceErrorDirectory == directory) ||
                               (detailError != nil && detailErrorDirectory == directory && detailErrorTab == tab &&
                                                         (tab != .branch || detailErrorBaseRef == baseRef)) {
                                Text("Showing the last available result.").font(.caption).foregroundStyle(.secondary)
                            }
                            Text(verbatim: workspace.branchLabel).font(.headline)
                            switch tab {
                            case .changes: changes(workspace)
                            case .branch: branch(workspace)
                            case .pullRequest: pullRequestDetails(workspace)
                            case .usage: EmptyView()
                            }
                            let readAt = tab == .branch ? (comparison?.baseRef == baseRef ? comparison?.readAt : nil) ?? workspace.readAt
                                : tab == .pullRequest ? pullRequest?.match?.readAt ?? workspace.readAt : workspace.readAt
                            Text("Read \(readAt.formatted(date: .abbreviated, time: .standard))")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(DesignTokens.Spacing.m)
            }
        }
        .background(.background)
        .task(id: WorkspaceRequest(directory: directory, refresh: refreshID, isActive: isActive,
                                   needsWorkspace: tab != .usage)) {
            await refreshWorkspace()
        }
        .task(id: DetailRequest(directory: directory, tab: tab, baseRef: baseRef,
                                revision: workspaceRevision, isActive: isActive)) {
            await refreshDetail()
        }
    }

    private func refreshWorkspace() async {
        guard isActive, tab != .usage else { return }
        loadingWorkspace = true
        workspaceError = nil
        do {
            let next = try await Git.inspect(in: directory)
            try Task.checkCancellation()
            workspace = next
            workspaceDirectory = directory
            workspaceRevision += 1
        } catch {
            guard !Task.isCancelled else { return }
            workspaceError = String(describing: error)
            workspaceErrorDirectory = directory
        }
        loadingWorkspace = false
    }

    private func refreshDetail() async {
        guard isActive, workspaceDirectory == directory, let workspace else { return }
        guard tab == .pullRequest || (tab == .branch && !baseRef.isEmpty && workspace.head != nil) else {
            loadingDetail = false
            detailError = nil
            return
        }
        let revision = workspaceRevision
        let selectedBaseRef = baseRef
        loadingDetail = true
        detailError = nil
        do {
            if tab == .branch {
                let result = try await Git.compareBranch(in: workspace, baseRef: selectedBaseRef)
                try Task.checkCancellation()
                guard workspaceRevision == revision, baseRef == selectedBaseRef else { return }
                comparison = result
                comparisonRevision = revision
            } else {
                let result = try await GitHubInspection().lookup(in: workspace)
                try Task.checkCancellation()
                guard workspaceRevision == revision else { return }
                pullRequest = result
                pullRequestRevision = revision
            }
        } catch {
            guard !Task.isCancelled, workspaceRevision == revision,
                  tab != .branch || baseRef == selectedBaseRef else { return }
            detailError = String(describing: error)
            detailErrorDirectory = directory
            detailErrorTab = tab
            detailErrorBaseRef = selectedBaseRef
        }
        loadingDetail = false
    }

    @ViewBuilder private func changes(_ workspace: GitWorkspaceSnapshot) -> some View {
        if workspace.head == nil { Text("This repository has no commits yet.").foregroundStyle(.secondary) }
        if workspace.files.isEmpty {
            Text("No local changes at the last refresh.").foregroundStyle(.secondary)
        }
        ForEach(GitChangeLayer.allCases.filter { $0 != .branch }, id: \.self) { layer in
            let files = workspace.files.filter { $0.layer == layer }
            if !files.isEmpty {
                Text("\(layer.rawValue) · \(files.count)").font(.subheadline.weight(.semibold))
                ForEach(files) { file in
                    fileButton(file) {
                        open(WorkspaceDocument(
                            title: file.path,
                            detail: "\(layer.rawValue) · \(workspace.branchLabel) · read on request"
                        ) { try await Git.localPatch(file, in: workspace) })
                    }
                }
            }
        }
    }

    @ViewBuilder private func branch(_ workspace: GitWorkspaceSnapshot) -> some View {
        if workspace.head == nil {
            Text("Commit a change before comparing branches.").foregroundStyle(.secondary)
        } else {
            Picker("Compare with", selection: $baseRef) {
                Text("Choose a base branch").tag("")
                ForEach(workspace.refs, id: \.self) { Text(verbatim: $0).tag($0) }
                if !baseRef.isEmpty, !workspace.refs.contains(baseRef) { Text("\(baseRef) (unavailable)").tag(baseRef) }
            }
            Text("Shows committed changes from the common base to HEAD. Local edits are in Changes.")
                .font(.caption).foregroundStyle(.secondary)
            if let comparison, comparison.baseRef == baseRef {
                if comparisonRevision != workspaceRevision {
                    Text("Showing the previous comparison while it updates.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Text(verbatim: "\(comparison.baseRef) @ \(comparison.baseOID.prefix(8)) → HEAD @ \(comparison.headOID.prefix(8))")
                    .font(.caption.monospaced()).textSelection(.enabled)
                    .help("Base \(comparison.baseOID)\nCommon base \(comparison.mergeBaseOID)\nHEAD \(comparison.headOID)")
                if comparison.files.isEmpty { Text("No committed changes in this comparison.").foregroundStyle(.secondary) }
                ForEach(comparison.files) { file in
                    fileButton(file) {
                        open(WorkspaceDocument(
                            title: file.path,
                            detail: "Committed snapshot · \(comparison.mergeBaseOID.prefix(8)) → \(comparison.headOID.prefix(8))"
                        ) { try await Git.branchPatch(file, comparison: comparison, in: workspace) })
                    }
                }
            }
        }
    }

    @ViewBuilder private func pullRequestDetails(_ workspace: GitWorkspaceSnapshot) -> some View {
        if let result = pullRequest {
            if pullRequestRevision != workspaceRevision {
                Text("Showing the previous PR result while it updates.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let match = result.match {
                let request = match.pullRequest
                Text("#\(request.number) · \(request.isDraft ? "DRAFT" : request.state)").font(.headline)
                Text(verbatim: request.title).textSelection(.enabled)
                Text(verbatim: "\(match.repository)\n\(request.baseRefName) @ \(request.baseRefOid.prefix(8)) ← \(request.headRefName) @ \(request.headRefOid.prefix(8))")
                    .font(.caption.monospaced()).textSelection(.enabled)
                Text("Local HEAD at refresh · \(match.localHead.prefix(8))").font(.caption.monospaced())
                if match.differsFromLocalHead {
                    Text("Local HEAD differs from the PR head. The PR diff shows GitHub's version.")
                        .font(.callout).foregroundStyle(.orange)
                }
                Text("Review · \(request.reviewDecision.flatMap { $0.isEmpty ? nil : $0 } ?? "Not reported")")
                    .font(.callout)
                if let checks = request.statusCheckRollup, !checks.isEmpty {
                    ForEach(Array(checks.enumerated()), id: \.offset) { _, check in
                        LabeledContent(check.label, value: check.result).font(.caption)
                    }
                } else { Text("No checks reported.").font(.caption).foregroundStyle(.secondary) }
                Button("Open PR diff") {
                    open(WorkspaceDocument(
                        title: "PR #\(request.number) · \(request.title)",
                        detail: "GitHub · \(match.repository) · \(request.baseRefOid.prefix(8)) ← \(request.headRefOid.prefix(8))"
                    ) { try await GitHubInspection().patch(for: match, in: workspace.root) })
                }
                if let url = URL(string: request.url) { Link("Open on GitHub", destination: url) }
            } else {
                Text("No open PR found in \(result.repositories.joined(separator: ", ")).").foregroundStyle(.secondary)
            }
        }
    }

    private func fileButton(_ file: GitChange, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(alignment: .firstTextBaseline) {
                Text(verbatim: file.path).lineLimit(2).truncationMode(.middle)
                Spacer(minLength: 4)
                Text(file.status).font(.caption.monospaced()).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .padding(.vertical, DesignTokens.Spacing.xxs)
        }
        .buttonStyle(.plain)
        .help(file.path)
        .accessibilityLabel("\(file.layer.rawValue) diff for \(file.path)")
    }
}

private struct UsageDetails: View {
    let usage: ChatUsage?
    let hasChat: Bool

    var body: some View {
        if !hasChat {
            Text("Select a chat to see usage.").foregroundStyle(.secondary)
        } else if let usage {
            Text("Current agent session").font(.headline)
            Text("Earlier model sessions have separate usage.").font(.caption).foregroundStyle(.secondary)
            if let context = usage.context {
                if let used = context.contextTokens {
                    if let capacity = context.contextCapacityTokens, let remaining = usage.remainingPercent {
                        Text("~\(remaining)% left · ~\(100 - remaining)% used").font(.title3)
                        ProgressView(value: min(Double(used), Double(capacity)), total: Double(capacity))
                        Text("\(used.formatted()) / \(capacity.formatted()) tokens").font(.callout.monospacedDigit())
                        if used > capacity { Text("Reported use exceeds the reported window.").foregroundStyle(.orange) }
                        Text("Uses the full reported window. The provider can reserve space and compact earlier.")
                            .font(.caption).foregroundStyle(.secondary)
                    } else {
                        Text("\(used.formatted()) context tokens").font(.title3)
                        Text("Capacity not reported; percentage unavailable.").font(.caption).foregroundStyle(.secondary)
                    }
                } else { Text(usage.contextNotice).foregroundStyle(.secondary) }
                Divider()
                Text("Last reported call · \(context.source.capitalized)").font(.subheadline.weight(.semibold))
                metric("Input, including cache", context.inputTokens)
                metric("Cache read", context.cacheReadTokens)
                metric("Cache write", context.cacheWriteTokens)
                metric("Output", context.outputTokens)
                Text(context.source == "codex" ? "Context includes input and output." : "Context includes input and cache; output is shown separately.")
                    .font(.caption).foregroundStyle(.secondary)
                timestamp(usage.contextTimestamp)
                if context.sessionInputTokens != nil || context.sessionOutputTokens != nil {
                    Divider()
                    Text("Provider session totals").font(.subheadline.weight(.semibold))
                    metric("Input", context.sessionInputTokens)
                    metric("Output", context.sessionOutputTokens)
                }
            } else { Text(usage.contextNotice).foregroundStyle(.secondary) }
            Divider()
            LabeledContent("Session cost", value: usage.costLabel ?? "Not reported")
            Text("Provider estimate in USD. Your bill can differ. Account limits are separate.")
                .font(.caption).foregroundStyle(.secondary)
            if usage.cost != nil { timestamp(usage.costTimestamp) }
            Text("Values update when the provider writes a report. They do not include the draft you are typing.")
                .font(.caption).foregroundStyle(.secondary)
        } else { Text("Waiting for usage data…").foregroundStyle(.secondary) }
    }

    private func metric(_ label: String, _ value: Int64?) -> some View {
        LabeledContent(label, value: value?.formatted() ?? "Not reported").font(.callout.monospacedDigit())
    }

    private func timestamp(_ value: String) -> some View {
        Text(value.isEmpty ? "Report time not supplied." : "Report time · \(value)")
            .font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
    }
}
