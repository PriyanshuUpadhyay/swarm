import AppKit
import SwiftUI
import SwarmCore

/// Find actions of the visible transcript, for the menu.
struct TranscriptFindActions {
    var open: () -> Void
    var next: () -> Void
    var previous: () -> Void
}

extension FocusedValues {
    @Entry var transcriptFindActions: TranscriptFindActions?
}

/// The chat transcript: a centered text column, find, history paging, tail following, and the
/// composer floating over its bottom edge. It gets the snapshot and closures, and owns only its
/// own scroll and find state.
struct TranscriptView<Composer: View>: View {
    let snapshot: ChairTranscriptSnapshot
    let revision: Int
    let hasOlder: Bool
    let isLoadingOlder: Bool
    let historyError: String?
    let waitingMessage: String
    let chair: String?
    let rawSessionJSON: String
    let isActive: Bool
    let isVisible: Bool
    /// Loads one older page.
    let loadOlder: () async -> Void
    let onTap: () -> Void
    var focus: FocusState<Bool>.Binding
    @ViewBuilder let composer: () -> Composer

    @State private var atLatest = true
    @State private var userScrolling = false
    @State private var nearOldest = false
    @State private var loadingHistory = false
    @State private var loadedHistoryThisGesture = false
    @State private var showHiddenRows = false
    @AppStorage("showRawData") private var showRawData = false
    @State private var findPresented = false
    @State private var findQuery = ""
    @State private var findMatches: [String] = []
    @State private var findMatchSet: Set<String> = []
    @State private var isSearching = false
    @State private var findMatchID: String?
    @State private var pendingScrollID: String?
    /// The owner's open or closed choice for each fold id; a fold without one takes its default.
    /// In memory for this chat only (ADR 0047).
    @State private var foldOverrides: [String: Bool] = [:]
    /// A find match inside a fold; the update that opens the fold scrolls to it.
    @State private var foldMatchID: String?
    @State private var composerHeight: CGFloat = 0
    /// The text and the composer use 90% of the chat page, centered.
    @State private var textWidth: CGFloat = 0
    @FocusState private var findFieldFocused: Bool

    private var rows: [TranscriptRow] {
        if case .rows(let rows, _) = snapshot { return rows }
        return []
    }

    private var rawEntries: [RawTranscriptEntry] {
        if case .rows(_, let raw) = snapshot { return raw }
        return []
    }

    var body: some View {
        VStack(spacing: 0) {
            if showRawData {
                HStack {
                    Spacer()
                    Text("RAW")
                        .font(.caption2.bold())
                        .padding(.horizontal, DesignTokens.Spacing.s)
                        .padding(.vertical, DesignTokens.Spacing.xxs)
                        .background(Capsule().fill(DesignTokens.rawBadgeFill))
                        .foregroundStyle(.orange)
                }
                .padding(.horizontal, DesignTokens.Spacing.m)
                .padding(.vertical, DesignTokens.Spacing.xs)
            }
            if findPresented { findBar }
            ScrollViewReader { proxy in
                scroll(proxy)
                    .overlay(alignment: .bottom) {
                        VStack(spacing: DesignTokens.Spacing.s) {
                            if !atLatest {
                                Button("Jump to latest", systemImage: "arrow.down") {
                                    // The list is upside down, so its logical top is the bottom edge.
                                    if let id = lastVisibleID { proxy.scrollTo(id, anchor: .top) }
                                }
                                .buttonStyle(.plain)
                                .font(.callout)
                                .padding(.horizontal, DesignTokens.Spacing.m)
                                .padding(.vertical, DesignTokens.Spacing.s)
                                .chromeSurface(in: Capsule())
                            }
                            composer()
                                .frame(width: textWidth > 0 ? textWidth : nil)
                                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { composerHeight = $0 }
                        }
                        .padding(DesignTokens.Spacing.m)
                    }
            }
            .frame(maxHeight: .infinity)
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width * 0.9 } action: { textWidth = $0 }
        .background(Color(nsColor: .textBackgroundColor))
        .focusable()
        .focusEffectDisabled()
        .focused(focus)
        // Scene-wide, so the menu finds the transcript without it holding keyboard focus.
        .background {
            if isVisible {
                Color.clear.focusedSceneValue(\.transcriptFindActions, TranscriptFindActions(
                    open: openFind, next: { stepFind(1) }, previous: { stepFind(-1) }
                ))
            }
        }
        .onChange(of: isVisible) { _, visible in
            if !visible { findFieldFocused = false }
        }
        // A click in the Find field selects this transcript, so Find Next steps it.
        .onChange(of: findFieldFocused) { _, focused in
            if focused { onTap() }
        }
        .task(id: searchRequest) {
            await updateSearch()
        }
    }

    /// The list is upside down (ADR 0028). Its first row is the newest, at the bottom edge, so
    /// older rows load at its end and new rows land at offset 0: neither moves the rows on screen.
    private func scroll(_ proxy: ScrollViewProxy) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                switch snapshot {
                case .loading:
                    DelayedProgress("Loading chat…").upsideDown()
                case .waiting:
                    Text(waitingMessage).foregroundStyle(.secondary).upsideDown()
                case .notice(let message):
                    Text(verbatim: message).foregroundStyle(.secondary).upsideDown()
                case .unavailable(let message):
                    Text(verbatim: message).foregroundStyle(.red).upsideDown()
                case .rows(_, let raw):
                    if showRawData {
                        ForEach(raw.reversed()) { entry in
                            rawEntry(entry).upsideDown()
                        }
                        rawSessionBlock.upsideDown()
                    } else {
                        ForEach(foldedLines.reversed()) { line in
                            switch line {
                            case .item(.row(let transcriptRow)):
                                rowView(transcriptRow).upsideDown()
                            case .item(.fold(let group)):
                                TranscriptRunFoldRow(rows: group, expanded: foldExpanded(line.id, rows: group))
                                    .upsideDown()
                            case .step(let transcriptRow):
                                // The list's spacing is m; an open fold keeps its steps xs apart.
                                rowView(transcriptRow)
                                    .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.s)
                                    .padding(.top, DesignTokens.Spacing.xs - DesignTokens.Spacing.m)
                                    .upsideDown()
                            }
                        }
                        let hidden = rows.filter(\.isHiddenByDefault).count
                        if hidden > 0 {
                            Button(showHiddenRows ? "Hide \(hidden) hidden rows" : "Show \(hidden) hidden rows") {
                                showHiddenRows.toggle()
                            }
                            .upsideDown()
                        }
                    }
                }
                if let historyError {
                    Text(verbatim: historyError).font(.caption).foregroundStyle(.red).upsideDown()
                }
                if hasOlder {
                    Button(isLoadingOlder ? "Loading earlier messages…" : "Load earlier messages") {
                        startLoadingOlder()
                    }
                    .disabled(isLoadingOlder)
                    .upsideDown()
                }
            }
            .font(DesignTokens.body)
            .lineSpacing(DesignTokens.bodyLineSpacing)
            .frame(width: textWidth > 0 ? textWidth : nil, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.vertical, DesignTokens.Spacing.l)
        }
        .upsideDown()
        // Upside down, the logical top is the bottom edge, where the composer floats over the rows.
        .contentMargins(.top, composerHeight + DesignTokens.Spacing.l, for: .scrollContent)
        .contentMargins(.bottom, DesignTokens.Spacing.s, for: .scrollContent)
        .frame(maxHeight: .infinity)
        .simultaneousGesture(TapGesture().onEnded {
            focus.wrappedValue = true
            onTap()
        })
        .onScrollGeometryChange(for: Bool.self) { $0.contentOffset.y <= 32 } action: { _, atLatest in
            self.atLatest = atLatest
        }
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentOffset.y + geometry.containerSize.height > geometry.contentSize.height - 120
        } action: { _, nearOldest in
            self.nearOldest = nearOldest
            if nearOldest, userScrolling { startLoadingOlder(automatic: true) }
        }
        .onScrollPhaseChange { _, phase in
            userScrolling = phase == .tracking || phase == .interacting || phase == .decelerating
            if phase == .idle { loadedHistoryThisGesture = false }
            if phase == .interacting, nearOldest { startLoadingOlder(automatic: true) }
        }
        .onChange(of: pendingScrollID) { _, id in
            guard let id else { return }
            pendingScrollID = nil
            if let fold = foldID(containing: id) {
                // A closed fold's steps are not list items yet, so open it and scroll in the next
                // update, when the list holds the step's id.
                foldOverrides[fold] = true
                foldMatchID = id
            } else {
                proxy.scrollTo(id, anchor: .center)
            }
        }
        // A failed step opens its live fold, which VoiceOver does not see by itself.
        .onChange(of: loadedLiveFailureID) { old, new in
            guard ToolRunFold.isNewFailure(from: old, to: new), isVisible else { return }
            AccessibilityNotification.Announcement("A step failed, so its steps are shown").post()
        }
        .onChange(of: foldMatchID) { _, id in
            guard let id else { return }
            foldMatchID = nil
            proxy.scrollTo(id, anchor: .center)
        }
    }

    private func rowView(_ transcriptRow: TranscriptRow) -> some View {
        TranscriptRowView(
            row: transcriptRow, chair: chair,
            revealForSearch: currentMatchID == transcriptRow.eventID,
            source: TranscriptSource.Lookup(ids: transcriptRow.sourceIDs) {
                TranscriptSource.entries(for: transcriptRow, in: rawEntries)
            }
        )
        .environment(\.transcriptSearchQuery, currentMatchID == transcriptRow.eventID ? findQuery : "")
        .padding(DesignTokens.Spacing.xxs)
        .background(matchBackground(transcriptRow.eventID))
    }

    /// The live failure once a snapshot is loaded, and nil before, so the first load is a baseline.
    private var loadedLiveFailureID: String?? {
        guard case .rows = snapshot else { return nil }
        return .some(ToolRunFold.liveFailureID(in: foldedItems, overrides: foldOverrides))
    }

    private var foldedItems: [ToolRunFold.Item] {
        ToolRunFold.items(in: visibleRows)
    }

    private var foldedLines: [ToolRunFold.Line] {
        ToolRunFold.lines(foldedItems) { ToolRunFold.isExpanded($1, overrides: foldOverrides) }
    }

    private func foldExpanded(_ id: String, rows: [TranscriptRow]) -> Binding<Bool> {
        Binding {
            ToolRunFold.isExpanded(rows, overrides: foldOverrides)
        } set: { open in
            foldOverrides[id] = open
        }
    }

    private func foldID(containing id: String) -> String? {
        foldedItems.first {
            if case .fold(let group) = $0 { group.contains { $0.eventID == id } } else { false }
        }?.id
    }

    private func startLoadingOlder(automatic: Bool = false) {
        guard hasOlder, !loadingHistory, !automatic || !loadedHistoryThisGesture else { return }
        loadedHistoryThisGesture = true
        loadingHistory = true
        Task { @MainActor in
            await loadOlder()
            loadingHistory = false
        }
    }

    private var findBar: some View {
        HStack(spacing: DesignTokens.Spacing.s) {
            TextField("Find in loaded messages", text: $findQuery)
                .textFieldStyle(.roundedBorder)
                .focused($findFieldFocused)
                .onSubmit { stepFind(1) }
                .onKeyPress(.return, phases: .down) { press in
                    guard press.modifiers.contains(.shift) else { return .ignored }
                    stepFind(-1)
                    return .handled
                }
                .onKeyPress(.escape) {
                    closeFind()
                    return .handled
                }
            Text(findCountText)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .fixedSize()
            Button("Previous match", systemImage: "chevron.up") { stepFind(-1) }
                .disabled(isSearching || findMatches.isEmpty)
            Button("Next match", systemImage: "chevron.down") { stepFind(1) }
                .disabled(isSearching || findMatches.isEmpty)
            Button("Close find", systemImage: "xmark", action: closeFind)
        }
        .labelStyle(.iconOnly)
        .buttonStyle(.borderless)
        .padding(.horizontal, DesignTokens.Spacing.m)
        .padding(.vertical, DesignTokens.Spacing.s)
        .background(.bar)
    }

    private var visibleRows: [TranscriptRow] {
        rows.filter { showHiddenRows || !$0.isHiddenByDefault }
    }

    private struct SearchRequest: Hashable {
        var query: String
        var revision: Int
        var raw: Bool
        var hidden: Bool
        var active: Bool
        var rawSession: String
    }

    private var searchRequest: SearchRequest {
        SearchRequest(
            query: findQuery, revision: revision, raw: showRawData,
            hidden: showHiddenRows, active: findPresented && isActive,
            rawSession: showRawData ? rawSessionJSON : ""
        )
    }

    private func updateSearch() async {
        let request = searchRequest
        guard request.active, !request.query.isEmpty else {
            findMatches = []
            findMatchSet = []
            findMatchID = nil
            isSearching = false
            return
        }
        isSearching = true
        let snapshot = snapshot
        let previous = findMatches
        let selected = findMatchID
        let search = Task.detached { () throws -> [String] in
            var items: [PaneSearchItem] = []
            if request.raw {
                items.append(PaneSearchItem(id: "raw-session", text: request.rawSession))
            }
            if case .rows(let rows, let raw) = snapshot {
                if request.raw {
                    for entry in raw {
                        try Task.checkCancellation()
                        items.append(PaneSearchItem(id: entry.id, text: entry.displayText))
                    }
                } else {
                    for row in rows where request.hidden || !row.isHiddenByDefault {
                        try Task.checkCancellation()
                        items.append(PaneSearchItem(id: row.eventID, text: row.searchText))
                    }
                }
            }
            try Task.checkCancellation()
            return PaneSearch.matches(query: request.query, in: items)
        }
        do {
            let matches = try await withTaskCancellationHandler {
                try await search.value
            } onCancel: {
                search.cancel()
            }
            guard !Task.isCancelled, request == searchRequest else { return }
            findMatches = matches
            findMatchSet = Set(matches)
            let index = PaneSearch.reconcile(
                current: selected.flatMap { previous.firstIndex(of: $0) },
                previousMatches: previous, newMatches: matches
            )
            findMatchID = index.map { matches[$0] }
            if selected != findMatchID { pendingScrollID = findMatchID }
            isSearching = false
        } catch {
            if !Task.isCancelled, request == searchRequest { isSearching = false }
        }
    }

    private var currentMatchID: String? {
        guard let findMatchID, findMatchSet.contains(findMatchID) else { return nil }
        return findMatchID
    }

    private var findIndex: Int? {
        findMatchID.flatMap { findMatches.firstIndex(of: $0) }
    }

    private var findCountText: String {
        if isSearching { return "Searching…" }
        guard let findIndex, !findMatches.isEmpty else { return "0 of 0" }
        return "\(findIndex + 1) of \(findMatches.count)"
    }

    private var lastVisibleID: String? {
        showRawData ? rawEntries.last?.id : visibleRows.last?.eventID
    }

    private var rawSessionBlock: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            Text("Session and agents").font(.caption).foregroundStyle(.secondary)
            TranscriptBoundedTextView(text: rawSessionJSON)
                .environment(\.transcriptSearchQuery, currentMatchID == "raw-session" ? findQuery : "")
        }
        .id("raw-session")
        .padding(DesignTokens.Spacing.s)
        .background(matchBackground("raw-session"))
    }

    private func rawEntry(_ entry: RawTranscriptEntry) -> some View {
        TranscriptRawEntryBlock(entry: entry)
            .environment(\.transcriptSearchQuery, currentMatchID == entry.id ? findQuery : "")
            .id(entry.id)
            .padding(DesignTokens.Spacing.s)
            .background(matchBackground(entry.id))
    }

    private func matchBackground(_ id: String) -> some ShapeStyle {
        guard findPresented else { return Color.clear }
        if currentMatchID == id { return DesignTokens.currentMatchFill }
        if findMatchSet.contains(id) { return DesignTokens.matchFill }
        return Color.clear
    }

    private func openFind() {
        findPresented = true
        resetFindSelection()
        Task { @MainActor in findFieldFocused = true }
    }

    private func closeFind() {
        findPresented = false
        findFieldFocused = false
        focus.wrappedValue = true
    }

    private func stepFind(_ delta: Int) {
        guard !isSearching else { return }
        let index = PaneSearch.step(current: findIndex, count: findMatches.count, delta: delta)
        findMatchID = index.map { findMatches[$0] }
        pendingScrollID = currentMatchID
    }

    private func resetFindSelection() {
        findMatchID = findMatches.first
        pendingScrollID = findMatches.first
    }
}

private struct TranscriptRowView: View {
    let row: TranscriptRow
    let chair: String?
    var revealForSearch = false
    let source: TranscriptSource.Lookup
    @State private var showingSource = false
    @State private var copying = false
    @State private var detailExpanded = false
    @State private var hovering = false

    var body: some View {
        Group {
            if let activity = row.tool {
                TranscriptToolCard(activity: activity, revealForSearch: revealForSearch)
            } else if row.kind == .divider {
                HStack(spacing: DesignTokens.Spacing.m) {
                    hairline
                    Text(verbatim: row.text).font(.caption).foregroundStyle(.secondary).fixedSize()
                    hairline
                }
                .padding(.vertical, DesignTokens.Spacing.s)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Context cleared at \(row.detail ?? "")")
            } else if row.systemKind == TranscriptSystemKind.swarmRing {
                // swarm typed it, not the owner, so it is a quiet line and not a "You" bubble.
                Label(TranscriptRow.swarmRingLine, systemImage: "envelope")
                    .font(.caption).foregroundStyle(.secondary)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(TranscriptRow.swarmRingLine)
                    .accessibilityIdentifier("transcript-swarm-ring")
            } else if let run = row.shell {
                TranscriptShellRow(run: run, revealForSearch: revealForSearch)
            } else if let command = row.command {
                // The owner typed the command, so it sits in their bubble.
                userBubble(TranscriptCommandChipView(chip: command, revealForSearch: revealForSearch))
            } else if row.kind == .user {
                userBubble(TranscriptMessageView(text: row.text))
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel("You")
            } else if row.endsTurn {
                turnEnd
            } else if row.kind == .assistant {
                // No visible label, so VoiceOver gets the speaker from the group, as "You" above.
                rowBody.padding(.vertical, DesignTokens.Spacing.xs)
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel(row.label(chair: chair))
            } else {
                rowBody
            }
        }
        // The user's bubble sits on the trailing edge, so its copy button goes in the free leading space.
        .overlay(alignment: row.kind == .user ? .topLeading : .topTrailing) {
            // Shown on hover only, so the transcript stays calm; VoiceOver has it as an action.
            if isMessage, hovering || copying {
                Button(copying ? "Copying…" : "Copy message", systemImage: "doc.on.doc", action: copy)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help("Copy message")
                    .disabled(copying)
                    .padding(DesignTokens.Spacing.s)
                    .accessibilityHidden(true)
            }
        }
        .onHover { hovering = $0 }
        .contextMenu {
            if isMessage { Button("Copy", action: copy) }
            Button("Show Source") { showingSource = true }
        }
        .accessibilityActions {
            if isMessage { Button("Copy message", action: copy) }
            Button("Show Source") { showingSource = true }
        }
        .popover(isPresented: $showingSource, arrowEdge: .leading) {
            TranscriptSourceView(entries: source.entries())
        }
        .id(row.eventID)
        .onChange(of: revealForSearch, initial: true) { _, reveal in
            if reveal { detailExpanded = true }
        }
    }

    private var rowBody: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            // Agent text has no label: the user bubble already sets the two voices apart.
            if row.kind != .assistant {
                Text(row.label(chair: chair))
                    .font(labelFont)
                    .foregroundStyle(.secondary)
            }
            switch row.kind {
            case .diff:
                if let diff = row.diff { TranscriptDiffView(diff: diff, revealForSearch: revealForSearch) }
            case .thought:
                DisclosureGroup("Show reasoning", isExpanded: $detailExpanded) {
                    TranscriptBoundedTextView(text: row.text)
                }
            case .toolResult:
                TranscriptOutputView(text: row.text,
                                     title: row.toolStatus == .failed ? "Failed tool result" : "Tool result",
                                     revealAll: revealForSearch)
            case .toolUse:
                DisclosureGroup(isExpanded: $detailExpanded) {
                    TranscriptBoundedTextView(text: row.detail ?? "")
                } label: {
                    Text(verbatim: row.text).font(DesignTokens.mono).lineLimit(1)
                }
            case .system where row.text.split(separator: "\n", maxSplits: 3).count > 3:
                DisclosureGroup(isExpanded: $detailExpanded) {
                    TranscriptBoundedTextView(text: row.text)
                } label: {
                    Text(verbatim: String(row.text.prefix(while: { !$0.isNewline })))
                        .font(DesignTokens.mono)
                        .lineLimit(1)
                }
            case .error:
                Text(verbatim: row.text).foregroundStyle(.red)
            case .assistant:
                TranscriptMessageView(text: row.text)
            default:
                Text(verbatim: row.text)
            }
        }
    }

    /// A turn-ended row or the interrupt notice: one quiet line, "Turn finished · 41s".
    private var turnEnd: some View {
        let stopped = row.kind == .notice || row.text == "aborted"
        let title = row.kind == .notice ? "Interrupted by you" : stopped ? "Turn interrupted" : "Turn finished"
        return Label([title, row.detail].compactMap { $0 }.joined(separator: " · "),
                     systemImage: stopped ? "stop.circle" : "checkmark.circle")
            .font(.caption).foregroundStyle(.secondary)
    }

    private var hairline: some View {
        Rectangle().fill(.quaternary).frame(height: DesignTokens.Size.hairline)
    }

    private func userBubble(_ content: some View) -> some View {
        UserBubbleLayout {
            content
                .padding(.horizontal, DesignTokens.Spacing.m)
                .padding(.vertical, DesignTokens.Spacing.s)
                .background(DesignTokens.userMessageFill, in: .rect(cornerRadius: DesignTokens.Radius.panel))
        }
    }

    private var isMessage: Bool { row.tool == nil && (row.kind == .user || row.kind == .assistant) }

    private func copy() {
        copying = true
        Task {
            try? await Task.sleep(for: .milliseconds(30))
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(row.text, forType: .string)
            copying = false
        }
    }

    private var labelFont: Font {
        switch row.kind {
        case .thought, .toolUse, .toolResult, .result, .system:
            DesignTokens.mono
        default:
            .caption
        }
    }
}

/// Fits the user's bubble to its text, at most `userBubbleMaxShare` of the column, on the trailing edge.
private struct UserBubbleLayout: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let bubble = subviews.first else { return .zero }
        let size = bubble.sizeThatFits(bubbleProposal(width: proposal.width, bubble))
        return CGSize(width: proposal.width ?? size.width, height: size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let bubble = subviews.first else { return }
        bubble.place(at: CGPoint(x: bounds.maxX, y: bounds.minY), anchor: .topTrailing,
                     proposal: bubbleProposal(width: bounds.width, bubble))
    }

    private func bubbleProposal(width: CGFloat?, _ bubble: LayoutSubview) -> ProposedViewSize {
        let ideal = bubble.sizeThatFits(.unspecified).width
        let limit = width.map { $0 * DesignTokens.userBubbleMaxShare } ?? ideal
        return ProposedViewSize(width: min(ideal, limit), height: nil)
    }
}

private extension View {
    /// Flips vertically; applied to the list and again to each row, so rows read the right way up.
    func upsideDown() -> some View {
        scaleEffect(x: 1, y: -1, anchor: .center)
    }
}

/// One raw event as RAW mode shows it: "[index] kind" and the pretty event JSON.
private struct TranscriptRawEntryBlock: View {
    let entry: RawTranscriptEntry

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            Text(verbatim: "[\(entry.index)] \(entry.rowKind)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            TranscriptBoundedTextView(text: entry.displayText)
        }
    }
}

/// Show Source: the translated event JSON of each event behind one row (ADR 0047).
private struct TranscriptSourceView: View {
    let entries: [RawTranscriptEntry]

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                Text(entries.isEmpty ? AttributedString("No source events are loaded for this row.")
                    : AttributedString(localized: "Source, ^[\(entries.count) event](inflect: true)"))
                    .font(.caption).foregroundStyle(.secondary)
                ForEach(entries) { TranscriptRawEntryBlock(entry: $0) }
            }
            .padding(DesignTokens.Spacing.m)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(width: DesignTokens.Size.profileSheet, height: DesignTokens.Size.sheet)
    }
}
