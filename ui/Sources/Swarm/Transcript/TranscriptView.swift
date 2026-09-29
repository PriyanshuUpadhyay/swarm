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

    @State private var followsTail = true
    @State private var userScrolling = false
    @State private var nearTop = false
    @State private var loadingHistory = false
    /// The row at the top edge. SwiftUI keeps it in place when older rows load above it.
    @State private var topRowID: String?
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
                            if !followsTail {
                                Button("Jump to latest", systemImage: "arrow.down") {
                                    followsTail = true
                                    if let id = lastVisibleID { proxy.scrollTo(id, anchor: .bottom) }
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
        .task(id: searchRequest) {
            await updateSearch()
        }
    }

    private func scroll(_ proxy: ScrollViewProxy) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                if hasOlder {
                    Button(isLoadingOlder ? "Loading earlier messages…" : "Load earlier messages") {
                        startLoadingOlder()
                    }
                    .disabled(isLoadingOlder)
                }
                if let historyError {
                    Text(verbatim: historyError).font(.caption).foregroundStyle(.red)
                }
                switch snapshot {
                case .loading:
                    DelayedProgress("Loading chat…")
                case .waiting:
                    Text(waitingMessage).foregroundStyle(.secondary)
                case .notice(let message):
                    Text(verbatim: message).foregroundStyle(.secondary)
                case .unavailable(let message):
                    Text(verbatim: message).foregroundStyle(.red)
                case .rows(let rows, let raw):
                    if showRawData {
                        rawSessionBlock
                        ForEach(raw) { entry in
                            rawEntry(entry)
                                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { old, height in
                                    guard entry.id == lastVisibleID, height > old else { return }
                                    followLatest(using: proxy)
                                }
                        }
                    } else {
                        let hidden = rows.filter(\.isHiddenByDefault).count
                        if hidden > 0 {
                            Button(showHiddenRows ? "Hide \(hidden) hidden rows" : "Show \(hidden) hidden rows") {
                                showHiddenRows.toggle()
                            }
                        }
                        ForEach(visibleRows) { transcriptRow in
                            TranscriptRowView(
                                row: transcriptRow, chair: chair,
                                revealForSearch: currentMatchID == transcriptRow.eventID
                            )
                            .environment(\.transcriptSearchQuery, currentMatchID == transcriptRow.eventID ? findQuery : "")
                            .padding(DesignTokens.Spacing.xxs)
                            .background(matchBackground(transcriptRow.eventID))
                            .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { old, height in
                                guard transcriptRow.eventID == lastVisibleID, height > old else { return }
                                followLatest(using: proxy)
                            }
                        }
                    }
                }
            }
            .scrollTargetLayout()
            .font(DesignTokens.body)
            .lineSpacing(DesignTokens.bodyLineSpacing)
            .frame(width: textWidth > 0 ? textWidth : nil, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.vertical, DesignTokens.Spacing.l)
        }
        .scrollPosition(id: $topRowID, anchor: .top)
        .defaultScrollAnchor(.bottom, for: .initialOffset)
        .contentMargins(.top, DesignTokens.Spacing.s, for: .scrollContent)
        // The composer floats over the last rows; this keeps them readable above it.
        .contentMargins(.bottom, composerHeight + DesignTokens.Spacing.l, for: .scrollContent)
        .frame(maxHeight: .infinity)
        .simultaneousGesture(TapGesture().onEnded {
            focus.wrappedValue = true
            onTap()
        })
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentOffset.y + geometry.containerSize.height
                >= geometry.contentSize.height + geometry.contentInsets.bottom - 32
        } action: { _, atBottom in
            followsTail = TranscriptTail.follows(
                current: followsTail, atBottom: atBottom, userScrolled: userScrolling
            )
        }
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentOffset.y < 120
        } action: { _, nearTop in
            self.nearTop = nearTop
            if nearTop, userScrolling { startLoadingOlder(automatic: true) }
        }
        .onScrollPhaseChange { _, phase, context in
            let wasScrolling = userScrolling
            userScrolling = phase == .tracking || phase == .interacting || phase == .decelerating
            if userScrolling {
                // Cancel tail following before layout or queued updates can move the viewport.
                followsTail = false
            } else if wasScrolling, phase == .idle {
                let geometry = context.geometry
                followsTail = geometry.contentOffset.y + geometry.containerSize.height
                    >= geometry.contentSize.height + geometry.contentInsets.bottom - 32
            }
            if phase == .idle { loadedHistoryThisGesture = false }
            if phase == .interacting, nearTop { startLoadingOlder(automatic: true) }
        }
        .onChange(of: rows) {
            guard !showRawData else { return }
            Task { @MainActor in followLatest(using: proxy) }
        }
        .onChange(of: rawEntries) {
            guard showRawData else { return }
            Task { @MainActor in followLatest(using: proxy) }
        }
        .onChange(of: pendingScrollID) { _, id in
            guard let id else { return }
            proxy.scrollTo(id, anchor: .center)
            pendingScrollID = nil
        }
    }

    private func startLoadingOlder(automatic: Bool = false) {
        guard hasOlder, !loadingHistory, !automatic || !loadedHistoryThisGesture else { return }
        loadedHistoryThisGesture = true
        loadingHistory = true
        followsTail = false
        Task { @MainActor in
            await loadOlder()
            loadingHistory = false
        }
    }

    private func followLatest(using proxy: ScrollViewProxy) {
        guard followsTail, !userScrolling, !loadingHistory, !findPresented, isVisible,
              let id = lastVisibleID else { return }
        proxy.scrollTo(id, anchor: .bottom)
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
                        let diff = row.tool?.diffs.map {
                            ([$0.path] + $0.hunks.flatMap(\.lines)).joined(separator: "\n")
                        }.joined(separator: "\n")
                        let text = [row.text, row.detail, row.tool?.command, row.tool?.output, row.tool?.path, diff]
                            .compactMap { $0 }.joined(separator: "\n")
                        items.append(PaneSearchItem(id: row.eventID, text: text))
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
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            Text("[\(entry.index)] \(entry.rowKind)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            TranscriptBoundedTextView(text: entry.displayText)
                .environment(\.transcriptSearchQuery, currentMatchID == entry.id ? findQuery : "")
        }
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
    @State private var copying = false
    @State private var detailExpanded = false
    @State private var hovering = false

    var body: some View {
        Group {
            if let activity = row.tool {
                TranscriptToolCard(title: row.text, activity: activity, revealForSearch: revealForSearch)
            } else if row.kind == .user {
                // A quiet tinted block, not a bubble.
                rowBody
                    .padding(DesignTokens.Spacing.m)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(DesignTokens.userMessageFill, in: .rect(cornerRadius: DesignTokens.Radius.card))
            } else if row.kind == .assistant {
                rowBody.padding(.vertical, DesignTokens.Spacing.xs)
            } else {
                rowBody
            }
        }
        .overlay(alignment: .topTrailing) {
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
        .accessibilityActions {
            if isMessage { Button("Copy message", action: copy) }
        }
        .id(row.eventID)
        .onChange(of: revealForSearch, initial: true) { _, reveal in
            if reveal { detailExpanded = true }
        }
    }

    private var rowBody: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            Text(row.label(chair: chair))
                .font(labelFont)
                .foregroundStyle(.secondary)
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
            case .user, .assistant:
                TranscriptMessageView(text: row.text)
            case .result where row.endsTurn:
                Label(row.text == "aborted" ? "Turn interrupted" : "Turn finished",
                      systemImage: row.text == "aborted" ? "stop.circle" : "checkmark.circle")
                    .font(.caption).foregroundStyle(.secondary)
            default:
                Text(verbatim: row.text)
            }
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
