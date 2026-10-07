import SwiftUI
import SwarmCore

struct StepRunRequest {
    let id = UUID()
    let run: StepRun
}

/// The Runs sidebar view: the step runs under `<workspace>/tmp/<skill>/`, and one run as a
/// top-down graph (ADR 0046). It reads files only; a node opens its step file in the preview.
struct StepRunsView: View {
    let directory: String
    var isActive = true
    let open: (WorkspaceDocument) -> Void
    var request: StepRunRequest? = nil
    var openedRun: () -> Void = {}
    var chatTitles: [String: String] = [:]
    var selectChat: (String) -> Void = { _ in }
    @State private var runs: [StepRun]?
    @State private var error: String?
    @State private var notice: String?
    @State private var scanNotice: String?
    /// The closed runs in `runs` come from this task's read, so an empty Closed group means none.
    @State private var closedRead = false
    /// The chosen run's last read, so the graph keeps it while the scan cannot read it or left it out.
    @State private var chosen: StepRun?
    @State private var showClosed = false
    @State private var retryID = 0
    @State private var graphFocusRequest: UUID?

    private struct Request: Equatable {
        let directory: String
        let isActive: Bool
        let showClosed: Bool
        let retryID: Int
    }

    var body: some View {
        Group {
            if let chosen {
                StepRunGraph(directory: directory, run: chosen, error: error, notice: graphNotice, open: open,
                             retry: { retryID += 1 }, back: { choose(nil) }, chatTitles: chatTitles, selectChat: selectChat,
                             focusRequest: isActive ? graphFocusRequest : nil, focusedTitle: { graphFocusRequest = nil })
            } else {
                list
            }
        }
        .onAppear {
            if request != nil { openRequest() } else { chosen = ChosenRun.byDirectory[directory] }
            // A chosen closed run shows only while the Closed group is read.
            if chosen?.closed == true { showClosed = true }
        }
        .onChange(of: request?.id) { _, _ in openRequest() }
        // The error and Retry appear in place, so VoiceOver hears them only if they are said, as SwitchModelSheet.
        .onChange(of: error) { _, text in
            if let text { AccessibilityNotification.Announcement(text).post() }
        }
        .onChange(of: graphNotice) { _, text in
            if let text { AccessibilityNotification.Announcement(text).post() }
        }
        .task(id: Request(directory: directory, isActive: isActive, showClosed: showClosed, retryID: retryID)) {
            guard isActive else { return }
            // Closed runs do not change, so they are read once when the group opens, not on every
            // tick (500 closed runs took about 4 s a scan). `ponytail:` a run closed while the group is
            // open shows there after the next toggle.
            var closed: StepRunScan?
            closedRead = false
            // The app has no file watcher; it polls live data, so a step change shows within 2 s.
            while !Task.isCancelled {
                do {
                    let readClosed = showClosed && closed == nil
                    var scan = try await StepRuns.scan(workspace: directory, includeClosed: readClosed)
                    try Task.checkCancellation()
                    if readClosed {
                        closed = scan
                    } else if let closed {
                        scan.addClosed(from: closed)
                    }
                    runs = scan.runs
                    if let run = scan.runs.first(where: { $0.id == chosen?.id }) { choose(run) }
                    scanNotice = scan.notice
                    closedRead = closed != nil
                    error = nil
                    if let chosen, StepRuns.isGone(chosen.id, closed: chosen.closed, from: scan, includeClosed: showClosed) {
                        choose(nil)
                        let text = "This run moved or was removed."
                        notice = text
                        // The graph closes under VoiceOver's focus, so the reason is spoken too.
                        AccessibilityNotification.Announcement(text).post()
                    }
                } catch is CancellationError {
                    return
                } catch {
                    self.error = String(describing: error)
                }
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    /// The scan notice when the graph shows the chosen run's last read, not this scan's read of it.
    private var graphNotice: String? {
        guard let chosen, let runs, !runs.contains(where: { $0.id == chosen.id }) else { return nil }
        return scanNotice
    }

    private func choose(_ run: StepRun?) {
        chosen = run
        ChosenRun.byDirectory[directory] = run
        notice = nil
    }

    private func openRequest() {
        guard let request else { return }
        choose(request.run)
        if request.run.closed { showClosed = true }
        graphFocusRequest = request.id
        AccessibilityNotification.Announcement("Opened run, \(request.run.name)").post()
        openedRun()
    }

    @ViewBuilder
    private var list: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Runs").font(.headline).padding(DesignTokens.Spacing.m).accessibilityAddTraits(.isHeader)
            Divider()
            if let error {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                    Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
                    Button("Retry") { retryID += 1 }
                }.padding(DesignTokens.Spacing.m)
            }
            if let notice {
                Text(notice).font(.caption).foregroundStyle(.secondary).padding(DesignTokens.Spacing.m)
            }
            if let scanNotice {
                Text(verbatim: scanNotice).font(.caption).foregroundStyle(.secondary).padding(DesignTokens.Spacing.m)
            }
            if let runs {
                let open = runs.filter { !$0.closed }
                if open.isEmpty && !showClosed && error == nil {
                    ContentUnavailableView(
                        "No step runs", systemImage: "point.3.connected.trianglepath.dotted",
                        description: Text("A skill such as flow writes its steps to tmp/<skill>/<run>/ in this workspace.")
                    )
                }
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                        ForEach(Dictionary(grouping: open, by: \.skill).sorted { $0.key < $1.key }, id: \.key) { skill, runs in
                            Text(verbatim: skill.uppercased()).font(.caption).foregroundStyle(.secondary)
                                .padding(.top, DesignTokens.Spacing.s)
                                .accessibilityAddTraits(.isHeader)
                            ForEach(runs) { run in StepRunRow(run: run) { choose(run) } }
                        }
                        DisclosureGroup("Closed", isExpanded: $showClosed) {
                            let closed = runs.filter(\.closed)
                            if !closedRead {
                                DelayedProgress("Reading closed runs…")
                            } else if closed.isEmpty {
                                Text("None").font(.caption)
                            }
                            ForEach(closed) { run in StepRunRow(run: run) { choose(run) } }
                        }
                        .foregroundStyle(.secondary)
                        .padding(.top, DesignTokens.Spacing.s)
                    }
                    .padding(DesignTokens.Spacing.m)
                }
            } else if error == nil {
                DelayedProgress("Reading runs…").padding(DesignTokens.Spacing.m)
            }
            Spacer(minLength: 0)
        }
    }
}

/// The chosen run's last read per workspace, kept while the app runs; the panels are rebuilt per
/// workspace, so a restored choice the first scan cannot read still has its last read.
private enum ChosenRun {
    @MainActor static var byDirectory: [String: StepRun] = [:]
}

private struct StepRunRow: View {
    let run: StepRun
    let choose: () -> Void

    var body: some View {
        Button(action: choose) {
            HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
                UrgencyGlyph(urgency: run.urgency).frame(width: DesignTokens.Size.glyphSlot)
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                    Text(verbatim: run.name).lineLimit(1).truncationMode(.middle)
                    Text(verbatim: summary).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
        }
        .buttonStyle(.plain).padding(.vertical, DesignTokens.Spacing.xxs)
        .help(run.id)
        .accessibilityLabel(run.spokenLabel)
    }

    private var summary: String {
        let counts = "\(run.doneCount) of \(run.steps.count)"
        return run.firstQuestion.map { "\(counts) · Waiting: \($0)" } ?? counts
    }
}

private struct UrgencyGlyph: View {
    let urgency: StepUrgency

    var body: some View {
        switch urgency {
        case .waiting: StatusGlyph(status: .waiting)
        case .blocked: StatusGlyph(status: .failed, title: "Blocked")
        case .stale: Image(systemName: "arrow.triangle.2.circlepath").foregroundStyle(.orange)
            .accessibilityLabel("Stale").help("Stale")
        case .active: StatusGlyph(status: .working)
        case .open: StatusGlyph(status: .ended, title: "Open")
        case .done: StatusGlyph(status: .done)
        }
    }
}

private struct StepRunGraph: View {
    let directory: String
    let run: StepRun
    let error: String?
    /// Set when the scan cannot read this run now, so the graph is its last read.
    let notice: String?
    let open: (WorkspaceDocument) -> Void
    let retry: () -> Void
    let back: () -> Void
    let chatTitles: [String: String]
    let selectChat: (String) -> Void
    let focusRequest: UUID?
    let focusedTitle: () -> Void
    @AccessibilityFocusState(for: .voiceOver) private var titleFocused: Bool

    private struct Edge: Identifiable {
        let from: String, to: String, dashed: Bool, stale: Bool
        var id: String { from + ">" + to }
    }

    private func focusTitle() async {
        // A cleared request acknowledges focus; it must not clear the title focus again.
        guard focusRequest != nil else { return }
        titleFocused = false
        await Task.yield()
        guard !Task.isCancelled else { return }
        titleFocused = true
        focusedTitle()
    }

    var body: some View {
        let layers = StepRuns.layers(run.steps)
        let layerOf = Dictionary(uniqueKeysWithValues: layers.enumerated().flatMap { index, ids in ids.map { ($0, index) } })
        let byID = Dictionary(run.steps.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        // An edge goes down only; one that closes a cycle (a hand edit) is not drawn.
        let edges = run.steps.flatMap { step in
            step.needs.compactMap { need -> Edge? in
                guard let from = layerOf[need], let to = layerOf[step.id], from < to else { return nil }
                let skipped = if case .skipped = step.state { true } else { false }
                return Edge(from: need, to: step.id, dashed: step.needsAssumed || skipped, stale: step.stale.contains(need))
            }
        }
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                Button("Runs", systemImage: "chevron.backward", action: back).buttonStyle(.borderless)
                Text(verbatim: run.skill).font(.caption).foregroundStyle(.secondary)
                Text(verbatim: run.name).font(.headline).lineLimit(2).truncationMode(.middle)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityFocused($titleFocused)
                    .task(id: focusRequest) { await focusTitle() }
                Text(verbatim: headline).font(.caption).foregroundStyle(.secondary)
                if let error {
                    Text(verbatim: "Showing the last read. \(error)").font(.caption).foregroundStyle(.red).textSelection(.enabled)
                    Button("Retry", action: retry).buttonStyle(.borderless).font(.caption)
                }
                if let notice { Text(verbatim: notice).font(.caption).foregroundStyle(.secondary) }
            }
            .padding(DesignTokens.Spacing.m)
            Divider()
            ScrollView {
                VStack(spacing: DesignTokens.Spacing.xl) {
                    ForEach(Array(layers.enumerated()), id: \.element) { _, ids in
                        HStack(alignment: .top, spacing: DesignTokens.Spacing.s) {
                            ForEach(ids, id: \.self) { id in
                                if let step = byID[id] {
                                    VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                                        StepNodeView(step: step) { preview(step) }
                                        if case .active = step.state, let title = chatTitles[step.path] {
                                            Button { selectChat(step.path) } label: {
                                                Label(title, systemImage: "bubble.left").lineLimit(1)
                                            }
                                            .buttonStyle(.plain).font(.caption).foregroundStyle(.secondary)
                                            .help("Show chat \(title)")
                                            .accessibilityLabel("Show chat, \(title)")
                                        }
                                    }
                                        .anchorPreference(key: NodeBounds.self, value: .bounds) { [id: $0] }
                                }
                            }
                        }
                    }
                }
                // A gutter on the left carries the edges that skip a layer, so no edge crosses a node.
                .padding(.leading, DesignTokens.Spacing.l)
                .backgroundPreferenceValue(NodeBounds.self) { bounds in
                    GeometryReader { proxy in
                        ForEach(edges) { edge in
                            if let from = bounds[edge.from].map({ proxy[$0] }), let to = bounds[edge.to].map({ proxy[$0] }) {
                                path(from: from, to: to, long: (layerOf[edge.to] ?? 0) - (layerOf[edge.from] ?? 0) > 1)
                                    .stroke(
                                        edge.stale ? Color.orange : Color.secondary,
                                        style: StrokeStyle(
                                            lineWidth: DesignTokens.Size.hairline,
                                            dash: edge.dashed || edge.stale ? [DesignTokens.Spacing.xs, DesignTokens.Spacing.xs] : []
                                        )
                                    )
                            }
                        }
                    }
                    .accessibilityHidden(true)
                }
                .padding(DesignTokens.Spacing.m)
            }
        }
    }

    private var headline: String {
        let waiting = run.steps.count { if case .waiting = $0.state { true } else { false } }
        let done = "\(run.doneCount) of \(run.steps.count) done"
        return waiting > 0 ? "\(done) · \(waiting) waiting" : done
    }

    private func path(from: CGRect, to: CGRect, long: Bool) -> Path {
        Path { path in
            if long {
                let gutter = min(from.minX, to.minX) - DesignTokens.Spacing.s
                path.move(to: CGPoint(x: from.minX, y: from.midY))
                path.addLine(to: CGPoint(x: gutter, y: from.midY))
                path.addLine(to: CGPoint(x: gutter, y: to.midY))
                path.addLine(to: CGPoint(x: to.minX, y: to.midY))
            } else {
                path.move(to: CGPoint(x: from.midX, y: from.maxY))
                path.addLine(to: CGPoint(x: to.midX, y: to.minY))
            }
        }
    }

    private func preview(_ step: StepNode) {
        let directory = directory
        open(WorkspaceDocument(title: step.path, detail: "Read-only · \(directory)", isDiff: false) {
            switch try await WorkspaceFiles.preview(in: directory, path: step.path) {
            case .text(let text), .notice(let text): return text
            }
        })
    }
}

private struct NodeBounds: PreferenceKey {
    static let defaultValue: [String: Anchor<CGRect>] = [:]
    static func reduce(value: inout [String: Anchor<CGRect>], nextValue: () -> [String: Anchor<CGRect>]) {
        value.merge(nextValue()) { first, _ in first }
    }
}

private struct StepNodeView: View {
    let step: StepNode
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.xs) {
                glyph.frame(width: DesignTokens.Size.glyphSlot)
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(verbatim: step.title).font(.callout.weight(.medium)).lineLimit(1)
                        Spacer(minLength: DesignTokens.Spacing.xs)
                        if let todo = step.todo {
                            Text(verbatim: "\(todo.checked)/\(todo.total)").font(.caption).monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                    }
                    if let detail {
                        Text(verbatim: detail.text).font(.caption).foregroundStyle(detail.color).lineLimit(4)
                    }
                    if !step.stale.isEmpty {
                        Label {
                            Text(verbatim: "Stale: \(step.stale.joined(separator: ", ")) changed")
                        } icon: {
                            Image(systemName: "arrow.triangle.2.circlepath")
                        }
                        .font(.caption).foregroundStyle(.orange)
                    }
                }
            }
            .padding(DesignTokens.Spacing.s)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
            .overlay {
                RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                    .strokeBorder(Color.secondary.opacity(DesignTokens.endedPaneOpacity), lineWidth: DesignTokens.Size.hairline)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(dimmed ? DesignTokens.endedPaneOpacity : 1)
        .help(help)
        .accessibilityLabel(step.spokenLabel)
    }

    private var dimmed: Bool { step.state == .open && !step.ready }

    @ViewBuilder
    private var glyph: some View {
        switch step.state {
        case .open: StatusGlyph(status: .ended, title: step.ready ? "Ready" : "Open")
        case .other(let word, _): StatusGlyph(status: .ended, title: word)
        case .active: StatusGlyph(status: .working)
        case .waiting: StatusGlyph(status: .waiting)
        case .blocked: StatusGlyph(status: .failed, title: "Blocked")
        case .unavailable: StatusGlyph(status: .failed, title: "Unavailable")
        case .done: StatusGlyph(status: .done)
        case .skipped: Image(systemName: "minus.circle").foregroundStyle(.secondary)
        case nil: Image(systemName: "questionmark.square.dashed").foregroundStyle(.red)
        }
    }

    private var detail: (text: String, color: Color)? {
        switch step.state {
        case .open: step.ready ? ("Ready", .secondary) : nil
        case .active(let agent): (agent, .secondary)
        case .waiting(let question): (question, .orange)
        case .blocked(let reason): ("Blocked: \(reason)", .red)
        case .unavailable(let tool): ("Needs \(tool)", .red)
        case .done: nil
        case .skipped: ("Skipped", .secondary)
        case .other(let word, let rest): ("\(word) \(rest)", .secondary)
        case nil: ("Can't read: \(step.error ?? "unknown")", .red)
        }
    }

    private var help: String {
        switch step.state {
        case .waiting(let question): question
        case .skipped(let reason): "Skipped: \(reason)"
        default: step.path
        }
    }
}
