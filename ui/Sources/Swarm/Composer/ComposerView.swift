import SwiftUI
import SwarmCore
import UniformTypeIdentifiers

/// The one app-facing entry point for message composition.
struct ComposerView: View {
    let sessionID: String
    var isActive = true
    var draft: Binding<String>
    let isRunning: Bool
    let isSending: Bool
    var modelLabel: String = "Choose model"
    var modelSwitchDisabledReason: String? = nil
    var selectModel: (() -> Void)? = nil
    var usageLabel: String? = nil
    var showUsage: (() -> Void)? = nil
    var sendDisabledReason: String? = nil
    var placeholder = "Message the chair"
    let commandSource: ComposerCommandSource
    let mentionSource: ComposerMentionSource
    let scratchDirectory: String
    var focus: FocusState<Bool>.Binding
    /// Receives the raw draft so the send owner can match it after the send.
    let send: (String) async throws -> Void
    let interrupt: () async throws -> Void
    let onFocused: () -> Void
    let isCurrentSession: () -> Bool

    @State private var commands: [ComposerCommand] = []
    @State private var files: [String] = []
    @State private var indexedSource: ComposerMentionSource?
    @State private var resolvedMenu: ComposerMenu = .none
    @State private var slashMatches: [ComposerCommandMatch] = []
    @State private var fileMatches: [ComposerFileMatch] = []
    @State private var selectedIndex = 0
    @State private var dismissedToken: ComposerToken?
    @State private var attachments: [ComposerAttachment] = []
    @State private var actionError: String?
    @State private var sendError: String?
    @State private var stopError: String?
    @State private var isDropTarget = false
    @State private var attachmentGeneration = 0
    @State private var pendingAttachments = 0
    @State private var isSubmitting = false
    @State private var isStopping = false
    @State private var matchGeneration = 0
    @State private var isMatchingFiles = false
    @State private var fileMatchTask: Task<[ComposerFileMatch], Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            if menuVisible { completionMenu }
            VStack(alignment: .leading, spacing: 0) {
                if !attachments.isEmpty { attachmentRow }
                editor
                footer
            }
            .chromeSurface(in: RoundedRectangle(cornerRadius: DesignTokens.Radius.panel, style: .continuous))
            .overlay {
                if isDropTarget {
                    RoundedRectangle(cornerRadius: DesignTokens.Radius.panel, style: .continuous)
                        .stroke(Color.accentColor, lineWidth: DesignTokens.Size.focusRing)
                }
            }
            if let actionError {
                Text(verbatim: actionError).font(.caption).foregroundStyle(.red)
            }
            if let sendError {
                Text(verbatim: sendError).font(.caption).foregroundStyle(.red)
            }
            if let stopError {
                Text(verbatim: stopError).font(.caption).foregroundStyle(.red)
            }
            if pendingAttachments > 0 {
                Text("Adding attachment…").font(.caption).foregroundStyle(.secondary)
            }
            if isSubmitting {
                Text("Sending…").font(.caption).foregroundStyle(.secondary)
            }
            if isStopping {
                Text("Stopping…").font(.caption).foregroundStyle(.secondary)
            }
            if let sendDisabledReason {
                Text(verbatim: sendDisabledReason).font(.caption).foregroundStyle(.secondary)
            }
        }
        .disabled(!isActive)
        .onDrop(
            of: [UTType.fileURL.identifier, UTType.image.identifier],
            isTargeted: $isDropTarget,
            perform: receiveDrop
        )
        .task(id: commandSource) {
            let discovered = await Task.detached {
                ComposerCommandCatalog.discover(from: commandSource)
            }.value
            guard !Task.isCancelled else { return }
            commands = discovered
            updateMatches()
        }
        .task(id: activeMentionSource) {
            guard let source = activeMentionSource, indexedSource != source else { return }
            files = []
            let discovered = await ComposerFileCatalog.discover(from: source)
            guard !Task.isCancelled else { return }
            files = discovered
            indexedSource = source
            updateMatches()
        }
        .onChange(of: draft.wrappedValue) {
            attachments = Composer.retainedAttachments(attachments, in: draft.wrappedValue)
            dismissedToken = nil
            selectedIndex = 0
            updateMatches()
        }
        .onChange(of: mentionSource) {
            files = []
            indexedSource = nil
            updateMatches()
        }
        .onChange(of: isActive) { _, active in
            if !active {
                attachmentGeneration += 1
                pendingAttachments = 0
                matchGeneration += 1
                fileMatchTask?.cancel()
            } else {
                updateMatches()
            }
        }
        .onAppear { updateMatches() }
        .onDisappear {
            attachmentGeneration += 1
            pendingAttachments = 0
            matchGeneration += 1
            fileMatchTask?.cancel()
        }
    }

    private var activeMentionSource: ComposerMentionSource? {
        if isActive, case .mention = resolvedMenu { return mentionSource }
        return nil
    }

    private var editor: some View {
        TextField(placeholder, text: draft, axis: .vertical)
            .onPasteCommand(of: [.png, .jpeg, .tiff], perform: receivePaste)
            .onKeyPress("v", phases: .down) { press in
                guard press.modifiers.contains(.command), pasteImage() else { return .ignored }
                return .handled
            }
            .lineLimit(1...8)
            .textFieldStyle(.plain)
            .font(DesignTokens.body)
            .focused(focus)
            .padding(.horizontal, DesignTokens.Spacing.m)
            .padding(.top, DesignTokens.Spacing.m)
            .padding(.bottom, DesignTokens.Spacing.s)
            .simultaneousGesture(TapGesture().onEnded {
                dismissMenu()
                focus.wrappedValue = true
                onFocused()
            })
            .onKeyPress(.leftArrow) { dismissMenuForCaretMove() }
            .onKeyPress(.rightArrow) { dismissMenuForCaretMove() }
            .onKeyPress(.upArrow) { handle(.up) }
            .onKeyPress(.downArrow) { handle(.down) }
            .onKeyPress(.tab) { handle(.tab) }
            .onKeyPress(.escape) { handle(.escape) }
            .onKeyPress(.return, phases: .down) { press in
                handle(press.modifiers.contains(.shift) ? .shiftReturn : .return)
            }
    }

    private var footer: some View {
        HStack(spacing: DesignTokens.Spacing.s) {
            if let selectModel {
                Button(action: selectModel) {
                    HStack(spacing: DesignTokens.Spacing.xs) {
                        Text(modelLabel).lineLimit(1).truncationMode(.middle)
                        Image(systemName: "chevron.down").font(.caption2)
                    }
                    .font(.callout)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Choose model: \(modelLabel)")
                .help(modelSwitchDisabledReason ?? "Choose a model for this chat")
                .disabled(modelSwitchDisabledReason != nil)
            }
            if let usageLabel, let showUsage {
                Button(action: showUsage) {
                    Text(usageLabel).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Usage details: \(usageLabel)")
                .help("Context, cache, and estimated cost for this agent session")
            }
            if showsStop {
                Text("⌘. stops")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            if showsStop {
                Button(action: stop) {
                    Image(systemName: isStopping ? "hourglass" : "stop.fill")
                        .frame(width: DesignTokens.Size.iconButton, height: DesignTokens.Size.iconButton)
                }
                .buttonStyle(.bordered)
                .buttonBorderShape(.circle)
                .tint(.red)
                .keyboardShortcut(".", modifiers: .command)
                .help("Stop the chair (⌘.)")
                .disabled(isStopping)
            }
            if !showsStop || Composer.outgoing(draft.wrappedValue) != nil {
                Button(action: submit) {
                    Image(systemName: "arrow.up")
                        .font(.headline)
                        .foregroundStyle(.background)
                        .frame(width: DesignTokens.Size.iconButton, height: DesignTokens.Size.iconButton)
                        .background(.primary, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
                }
                .buttonStyle(.plain)
                .disabled(Composer.outgoing(draft.wrappedValue) == nil || isSending
                    || isSubmitting || pendingAttachments > 0 || sendDisabledReason != nil)
                .help("Send (Return)")
            }
        }
        .padding(.horizontal, DesignTokens.Spacing.m)
        .padding(.bottom, DesignTokens.Spacing.s)
    }

    private var attachmentRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: DesignTokens.Spacing.s) {
                ForEach(attachments) { attachment in
                    HStack(spacing: DesignTokens.Spacing.xs) {
                        Image(systemName: ComposerAttachmentStore.isImage(
                            pathExtension: (attachment.path as NSString).pathExtension
                        ) ? "photo" : "doc")
                        Text(attachment.name).lineLimit(1)
                        Button {
                            draft.wrappedValue = Composer.removing(
                                path: attachment.path, from: draft.wrappedValue
                            )
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Remove \(attachment.name)")
                    }
                    .font(.caption)
                    .padding(.horizontal, DesignTokens.Spacing.s)
                    .padding(.vertical, DesignTokens.Spacing.xs)
                    .background(.quaternary, in: Capsule())
                }
            }
            .padding(.horizontal, DesignTokens.Spacing.m)
            .padding(.top, DesignTokens.Spacing.s)
        }
    }

    @ViewBuilder
    private var completionMenu: some View {
        VStack(alignment: .leading, spacing: 0) {
            if completionCount == 0 {
                Text(emptyMenuText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(DesignTokens.Spacing.m)
            } else {
                ForEach(0..<completionCount, id: \.self) { index in
                    Button { pick(index) } label: {
                        completionRow(index)
                            .padding(.horizontal, DesignTokens.Spacing.m)
                            .padding(.vertical, DesignTokens.Spacing.s)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(index == selectedIndex ? DesignTokens.currentMatchFill : .clear)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .frame(maxWidth: DesignTokens.Size.menuWidth)
        .chromeSurface(in: RoundedRectangle(cornerRadius: DesignTokens.Radius.card, style: .continuous))
    }

    private func completionRow(_ index: Int) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
            Text(verbatim: completionName(index)).font(DesignTokens.mono)
            Text(verbatim: completionDetail(index))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            Spacer(minLength: 0)
        }
    }

    // TextField exposes no selection, so completion works only at the end of the draft.
    private func updateMatches() {
        guard isActive else { return }
        let text = draft.wrappedValue
        resolvedMenu = ComposerMenu.resolve(draft: text, caret: (text as NSString).length)
        matchGeneration += 1
        fileMatchTask?.cancel()
        fileMatchTask = nil
        switch resolvedMenu {
        case .none:
            slashMatches = []
            fileMatches = []
            isMatchingFiles = false
        case .slash(let token):
            slashMatches = ComposerCommandCatalog.matches(commands, query: token.query)
            fileMatches = []
            isMatchingFiles = false
        case .mention(let token):
            slashMatches = []
            fileMatches = []
            isMatchingFiles = !files.isEmpty || indexedSource != mentionSource
            guard !files.isEmpty else { return }
            let currentGeneration = matchGeneration
            let paths = files
            let query = token.query
            let task = Task.detached(priority: .userInitiated) {
                ComposerFileCatalog.matches(paths, query: query)
            }
            fileMatchTask = task
            Task {
                let matches = await task.value
                guard !task.isCancelled, currentGeneration == matchGeneration,
                      isCurrentSession() else { return }
                fileMatches = matches
                isMatchingFiles = false
                fileMatchTask = nil
            }
        }
    }
    private var menuVisible: Bool {
        guard let token = resolvedMenu.token else { return false }
        return token != dismissedToken
    }
    private var completionCount: Int {
        switch resolvedMenu {
        case .none: 0
        case .slash: slashMatches.count
        case .mention: fileMatches.count
        }
    }
    private var emptyMenuText: String {
        switch resolvedMenu {
        case .none: ""
        case .slash: "No command matches"
        case .mention: isMatchingFiles ? "Finding files…" : "No file matches"
        }
    }

    private func completionName(_ index: Int) -> String {
        switch resolvedMenu {
        case .none: ""
        case .slash: "/" + slashMatches[index].command.name
        case .mention: "@" + fileMatches[index].path
        }
    }

    private func completionDetail(_ index: Int) -> String {
        switch resolvedMenu {
        case .none, .mention: ""
        case .slash: slashMatches[index].command.detail
        }
    }

    private func handle(_ key: ComposerInputKey) -> KeyPress.Result {
        let action = ComposerKeyRouter.route(
            key, menuOpen: menuVisible, hasRows: completionCount > 0
        )
        switch action {
        case .move(let delta):
            guard menuVisible, completionCount > 0 else { return .ignored }
            selectedIndex = ComposerKeyRouter.movedSelection(
                current: selectedIndex, count: completionCount, delta: delta
            )
        case .pick:
            guard completionCount > 0 else { return .ignored }
            pick(min(selectedIndex, completionCount - 1))
        case .dismissMenu:
            dismissMenu()
        case .clear:
            attachmentGeneration += 1
            pendingAttachments = 0
            draft.wrappedValue = ""
        case .send:
            submit()
        case .insertNewline:
            draft.wrappedValue.append("\n")
        }
        return .handled
    }

    private func pick(_ index: Int) {
        guard let token = resolvedMenu.token else { return }
        switch resolvedMenu {
        case .none:
            return
        case .slash:
            draft.wrappedValue = ComposerMenu.inserting(
                "/" + slashMatches[index].command.name,
                into: draft.wrappedValue,
                token: token
            )
        case .mention:
            draft.wrappedValue = ComposerMenu.inserting(
                "@" + fileMatches[index].path,
                into: draft.wrappedValue,
                token: token
            )
        }
        selectedIndex = 0
        focus.wrappedValue = true
    }

    private func dismissMenuForCaretMove() -> KeyPress.Result {
        dismissMenu()
        return .ignored
    }

    private func dismissMenu() {
        if menuVisible { dismissedToken = resolvedMenu.token }
    }

    private var showsStop: Bool { isRunning }

    private func submit() {
        let snapshot = draft.wrappedValue
        guard !isSending, !isSubmitting, pendingAttachments == 0, sendDisabledReason == nil,
              Composer.outgoing(snapshot) != nil else {
            return
        }
        attachmentGeneration += 1
        pendingAttachments = 0
        isSubmitting = true
        sendError = nil
        Task {
            defer { isSubmitting = false }
            do {
                try await send(snapshot)
                sendError = nil
                if isCurrentSession() { focus.wrappedValue = true }
            } catch {
                sendError = (error as? SwarmProfileError)?.message ?? String(describing: error)
            }
        }
    }

    private func stop() {
        guard !isStopping else { return }
        isStopping = true
        stopError = nil
        Task {
            defer { isStopping = false }
            do {
                try await interrupt()
                stopError = nil
            } catch {
                stopError = (error as? SwarmProfileError)?.message ?? String(describing: error)
            }
        }
    }

    private func pasteImage() -> Bool {
        let clipboard = NSPasteboard.general
        let types: [(NSPasteboard.PasteboardType, String)] = [(.png, "png"), (.init(UTType.jpeg.identifier), "jpg"), (.tiff, "tiff")]
        guard let (type, fileExtension) = types.first(where: { clipboard.types?.contains($0.0) == true }) else {
            return false
        }
        let context = attachmentContext
        let version = clipboard.changeCount
        pendingAttachments += 1
        actionError = nil
        Task {
            // The native text field consumes image paste before the enclosing paste command.
            // Give feedback before asking the clipboard owner to supply its image bytes.
            try? await Task.sleep(for: .milliseconds(30))
            guard isCurrent(context) else { return }
            guard clipboard.changeCount == version, let data = clipboard.data(forType: type) else {
                pendingAttachments -= 1
                actionError = "Cannot read pasted image."
                return
            }
            await addImage(data, fileExtension: fileExtension, context: context)
        }
        return true
    }

    private func receivePaste(_ providers: [NSItemProvider]) {
        for provider in providers {
            _ = loadImage(from: provider)
        }
    }

    private func receiveDrop(_ providers: [NSItemProvider]) -> Bool {
        let context = attachmentContext
        var accepted = false
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                accepted = true
                pendingAttachments += 1
                actionError = nil
                provider.loadDataRepresentation(forTypeIdentifier: UTType.fileURL.identifier) { data, error in
                    Task { @MainActor in
                        guard isCurrent(context) else { return }
                        guard let data, let url = URL(dataRepresentation: data, relativeTo: nil) else {
                            pendingAttachments -= 1
                            actionError = error?.localizedDescription ?? "Cannot read dropped file."
                            return
                        }
                        await addFile(at: url.path, context: context)
                    }
                }
            } else if loadImage(from: provider) {
                accepted = true
            }
        }
        return accepted
    }

    private func loadImage(from provider: NSItemProvider) -> Bool {
        let context = attachmentContext
        let types: [(UTType, String)] = [(.png, "png"), (.jpeg, "jpg"), (.tiff, "tiff")]
        guard let item = types.first(where: {
            provider.hasItemConformingToTypeIdentifier($0.0.identifier)
        }) else { return false }
        pendingAttachments += 1
        actionError = nil
        provider.loadDataRepresentation(forTypeIdentifier: item.0.identifier) { data, error in
            Task { @MainActor in
                guard isCurrent(context) else { return }
                if let data {
                    await addImage(data, fileExtension: item.1, context: context)
                } else {
                    pendingAttachments -= 1
                    actionError = error?.localizedDescription ?? "Cannot read pasted image."
                }
            }
        }
        return true
    }

    private func addImage(
        _ data: Data, fileExtension: String, context: ComposerAttachmentContext
    ) async {
        let directory = scratchDirectory
        do {
            let attachment = try await Task.detached(priority: .userInitiated) {
                try ComposerAttachmentStore.saveImage(
                    data, fileExtension: fileExtension, scratchDirectory: directory
                )
            }.value
            guard isCurrent(context) else { return }
            pendingAttachments -= 1
            add(attachment)
        } catch {
            guard isCurrent(context) else { return }
            pendingAttachments -= 1
            actionError = error.localizedDescription
        }
    }

    private func addFile(at path: String, context: ComposerAttachmentContext) async {
        let directory = scratchDirectory
        do {
            let attachment = try await Task.detached(priority: .userInitiated) {
                try ComposerAttachmentStore.importFile(at: path, scratchDirectory: directory)
            }.value
            guard isCurrent(context) else { return }
            pendingAttachments -= 1
            add(attachment)
        } catch {
            guard isCurrent(context) else { return }
            pendingAttachments -= 1
            actionError = error.localizedDescription
        }
    }

    private func add(_ attachment: ComposerAttachment) {
        if !attachments.contains(attachment) { attachments.append(attachment) }
        draft.wrappedValue = Composer.appending(path: attachment.path, to: draft.wrappedValue)
        focus.wrappedValue = true
    }

    private var attachmentContext: ComposerAttachmentContext {
        ComposerAttachmentContext(
            sessionID: sessionID, generation: attachmentGeneration
        )
    }

    private func isCurrent(_ context: ComposerAttachmentContext) -> Bool {
        isCurrentSession() && context.matches(
            sessionID: sessionID, generation: attachmentGeneration
        )
    }
}
