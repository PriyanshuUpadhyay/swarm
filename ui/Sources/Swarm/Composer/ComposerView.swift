import SwiftUI
import SwarmCore
import UniformTypeIdentifiers

/// The one app-facing entry point for message composition.
struct ComposerView: View {
    let sessionID: String
    var draft: Binding<String>
    let isRunning: Bool
    let isSending: Bool
    let commandSource: ComposerCommandSource
    let mentionSource: ComposerMentionSource
    let scratchDirectory: String
    var focus: FocusState<Bool>.Binding
    /// Receives the raw draft so the send owner can match it after the send.
    let send: (String) async throws -> Void
    let interrupt: () async throws -> Void
    let onFocused: () -> Void

    @State private var commands: [ComposerCommand] = []
    @State private var files: [String] = []
    @State private var selectedIndex = 0
    @State private var dismissedToken: ComposerToken?
    @State private var attachments: [ComposerAttachment] = []
    @State private var actionError: String?
    @State private var isDropTarget = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if menuVisible { completionMenu }
            VStack(alignment: .leading, spacing: 0) {
                if !attachments.isEmpty { attachmentRow }
                editor
                footer
            }
            .background(.background)
            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .stroke(isDropTarget ? Color.accentColor : Color.secondary.opacity(0.25))
            }
            if let actionError {
                Text(verbatim: actionError).font(.caption).foregroundStyle(.red)
            }
        }
        .onDrop(
            of: [UTType.fileURL.identifier, UTType.image.identifier],
            isTargeted: $isDropTarget,
            perform: receiveDrop
        )
        .onPasteCommand(of: [.png, .jpeg, .tiff], perform: receivePaste)
        .task(id: commandSource) {
            commands = await Task.detached {
                ComposerCommandCatalog.discover(from: commandSource)
            }.value
        }
        .task(id: mentionSource) {
            files = await ComposerFileCatalog.discover(from: mentionSource)
        }
        .onChange(of: draft.wrappedValue) {
            attachments = Composer.retainedAttachments(attachments, in: draft.wrappedValue)
            dismissedToken = nil
            selectedIndex = 0
        }
        .onChange(of: sessionID) {
            attachments = []
            actionError = nil
            dismissedToken = nil
            selectedIndex = 0
        }
    }

    private var editor: some View {
        TextField("Message the chair", text: draft, axis: .vertical)
            .lineLimit(1...8)
            .textFieldStyle(.plain)
            .font(.body)
            .focused(focus)
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 8)
            .simultaneousGesture(TapGesture().onEnded {
                if menuVisible { dismissedToken = resolvedMenu.token }
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
        HStack(spacing: 8) {
            if showsStop {
                Text("⌘. stops")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            if showsStop {
                Button(action: stop) {
                    Image(systemName: "stop.fill")
                        .frame(width: 26, height: 26)
                }
                .buttonStyle(.bordered)
                .buttonBorderShape(.circle)
                .tint(.red)
                .keyboardShortcut(".", modifiers: .command)
                .help("Stop the chair (⌘.)")
            }
            if !showsStop || Composer.outgoing(draft.wrappedValue) != nil {
                Button(action: submit) {
                    Image(systemName: "arrow.up")
                        .font(.headline)
                        .foregroundStyle(.background)
                        .frame(width: 26, height: 26)
                        .background(.primary, in: RoundedRectangle(cornerRadius: 7))
                }
                .buttonStyle(.plain)
                .disabled(Composer.outgoing(draft.wrappedValue) == nil || isSending)
                .help("Send (Return)")
            }
        }
        .padding(.horizontal, 10)
        .padding(.bottom, 8)
    }

    private var attachmentRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(attachments) { attachment in
                    HStack(spacing: 5) {
                        Image(systemName: "photo")
                        Text(attachment.name).lineLimit(1)
                        Button {
                            attachments.removeAll { $0 == attachment }
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
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(.quaternary, in: Capsule())
                }
            }
            .padding(.horizontal, 10)
            .padding(.top, 8)
        }
    }

    @ViewBuilder
    private var completionMenu: some View {
        VStack(alignment: .leading, spacing: 0) {
            if completionCount == 0 {
                Text(emptyMenuText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(10)
            } else {
                ForEach(0..<completionCount, id: \.self) { index in
                    Button { pick(index) } label: {
                        completionRow(index)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 7)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(index == selectedIndex ? Color.accentColor.opacity(0.18) : .clear)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .frame(maxWidth: 480)
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: 10).stroke(.separator) }
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
    }

    private func completionRow(_ index: Int) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(verbatim: completionName(index)).font(.system(.body, design: .monospaced))
            Text(verbatim: completionDetail(index))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            Spacer(minLength: 0)
        }
    }

    // TextField exposes no selection, so completion works only at the end of the draft.
    private var caret: Int { (draft.wrappedValue as NSString).length }
    private var resolvedMenu: ComposerMenu {
        ComposerMenu.resolve(draft: draft.wrappedValue, caret: caret)
    }
    private var menuVisible: Bool {
        guard let token = resolvedMenu.token else { return false }
        return token != dismissedToken
    }
    private var slashMatches: [ComposerCommandMatch] {
        guard case .slash(let token) = resolvedMenu else { return [] }
        return ComposerCommandCatalog.matches(commands, query: token.query)
    }
    private var fileMatches: [ComposerFileMatch] {
        guard case .mention(let token) = resolvedMenu else { return [] }
        return ComposerFileCatalog.matches(files, query: token.query)
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
        case .mention: "No file matches"
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
            dismissedToken = resolvedMenu.token
        case .clear:
            draft.wrappedValue = ""
            attachments = []
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
        if menuVisible { dismissedToken = resolvedMenu.token }
        return .ignored
    }

    private var showsStop: Bool { isRunning }

    private func submit() {
        let snapshot = draft.wrappedValue
        guard !isSending, Composer.outgoing(snapshot) != nil else {
            return
        }
        Task {
            do {
                try await send(snapshot)
                actionError = nil
                focus.wrappedValue = true
            } catch {
                actionError = String(describing: error)
            }
        }
    }

    private func stop() {
        Task {
            do {
                try await interrupt()
                actionError = nil
            } catch {
                actionError = String(describing: error)
            }
        }
    }

    private func receivePaste(_ providers: [NSItemProvider]) {
        for provider in providers {
            if loadImage(from: provider) { return }
        }
    }

    private func receiveDrop(_ providers: [NSItemProvider]) -> Bool {
        var accepted = false
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                accepted = true
                provider.loadDataRepresentation(forTypeIdentifier: UTType.fileURL.identifier) { data, _ in
                    guard let data, let url = URL(dataRepresentation: data, relativeTo: nil) else { return }
                    Task { @MainActor in addFile(at: url.path) }
                }
            } else if loadImage(from: provider) {
                accepted = true
            }
        }
        return accepted
    }

    private func loadImage(from provider: NSItemProvider) -> Bool {
        let types: [(UTType, String)] = [(.png, "png"), (.jpeg, "jpg"), (.tiff, "tiff")]
        guard let item = types.first(where: {
            provider.hasItemConformingToTypeIdentifier($0.0.identifier)
        }) else { return false }
        provider.loadDataRepresentation(forTypeIdentifier: item.0.identifier) { data, error in
            Task { @MainActor in
                if let data {
                    addImage(data, fileExtension: item.1)
                } else if let error {
                    actionError = String(describing: error)
                }
            }
        }
        return true
    }

    private func addImage(_ data: Data, fileExtension: String) {
        do {
            add(try ComposerAttachmentStore.saveImage(
                data, fileExtension: fileExtension, scratchDirectory: scratchDirectory
            ))
        } catch {
            actionError = String(describing: error)
        }
    }

    private func addFile(at path: String) {
        do {
            add(try ComposerAttachmentStore.importFile(
                at: path, scratchDirectory: scratchDirectory
            ))
        } catch {
            actionError = String(describing: error)
        }
    }

    private func add(_ attachment: ComposerAttachment) {
        if !attachments.contains(attachment) { attachments.append(attachment) }
        draft.wrappedValue = Composer.appending(path: attachment.path, to: draft.wrappedValue)
        focus.wrappedValue = true
    }
}
