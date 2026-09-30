import SwiftUI
import SwarmCore

/// Edits one profile's runners as numbered cards: add, remove, reorder, and each runner's
/// provider, model, effort, and the flags its provider takes. Save writes the whole profile in
/// one call; Cancel drops every edit.
struct ProfileEditorSheet: View {
    let providers: [SwarmProvider]
    /// Why the provider list is empty, when its read failed.
    let providersError: String?
    /// The last launch check of the saved profile, for each card's status.
    let check: SwarmProfileCheck?
    /// The runner to scroll to when a chip opened the sheet.
    let focus: Int?
    let save: (SwarmProfile) async throws -> Void
    let onSaved: () -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var draft: ProfileDraft
    @State private var models: [String: [SwarmModel]] = [:]
    @State private var catalogErrors: [String: String] = [:]
    @State private var isSaving = false
    @State private var error: String?
    /// The card list's content height as the list lays it out, so the list fits its cards.
    @State private var contentHeight: CGFloat?

    init(
        profile: SwarmProfile, providers: [SwarmProvider], providersError: String?,
        check: SwarmProfileCheck?, focus: Int?, save: @escaping (SwarmProfile) async throws -> Void, onSaved: @escaping () -> Void
    ) {
        self.providers = providers
        self.providersError = providersError
        self.check = check
        self.focus = focus
        self.save = save
        self.onSaved = onSaved
        // A one-time seed: the sheet owns the edits from here until Save or Cancel.
        _draft = State(initialValue: ProfileDraft(profile))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                Text("Edit profile · \(draft.original.name)").font(.title2)
                Text("The first runner that can run starts. A runner is skipped when its CLI is missing, no account is signed in, or usage is low. Drag a card to change the order.")
                    .font(.callout).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ScrollViewReader { proxy in
                List {
                    ForEach($draft.runners) { $runner in
                        let index = draft.runners.firstIndex { $0.id == runner.id } ?? 0
                        RunnerCard(
                            index: index,
                            runner: $runner,
                            provider: provider(runner.provider),
                            providers: providers,
                            models: models[runner.provider] ?? [],
                            catalogError: catalogErrors[runner.provider],
                            status: draft.cardStatus(at: index, check: check),
                            blocked: check != nil && check?.pick == nil,
                            canRemove: draft.canRemove,
                            isLast: index == draft.runners.count - 1,
                            onProvider: { choice in
                                Task { await changeProvider(choice, at: index) }
                            },
                            onRemove: { draft.remove(at: index) },
                            onMove: { offset in move(index, by: offset) },
                            onRetry: { Task { await loadModels(runner.provider, retry: true) } }
                        )
                        .id(runner.id)
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                    }
                    .onMove { draft.move(fromOffsets: $0, toOffset: $1) }
                }
                .scrollContentBackground(.hidden)
                .onScrollGeometryChange(for: CGFloat.self) { $0.contentSize.height } action: { _, height in
                    if height > 0 { contentHeight = height }
                }
                // The sheet keeps the size it opened at, so when cards are added the list gives up
                // height and scrolls rather than push the title and buttons out.
                .frame(minHeight: DesignTokens.Size.runnerCard, idealHeight: listHeight, maxHeight: listHeight)
                .onChange(of: draft.runners.count) { old, new in
                    if new > old, let added = draft.runners.last { proxy.scrollTo(added.id, anchor: .bottom) }
                }
                .onAppear {
                    if let focus, draft.runners.indices.contains(focus) {
                        proxy.scrollTo(draft.runners[focus].id, anchor: .center)
                    }
                }
            }
            Button {
                draft.add(from: providers) { models[$0]?.first?.id }
                if let added = draft.runners.last {
                    Task {
                        await loadModels(added.provider)
                        fillEmptyModel(of: added.id)
                    }
                }
            } label: {
                Label("Add runner", systemImage: "plus")
            }
            .buttonStyle(.borderless)
            .disabled(providers.isEmpty)
            if providers.isEmpty {
                Text(verbatim: "Add runner is off because the provider list could not be read. "
                    + (providersError ?? "Close the editor and try again."))
                    .font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let error {
                Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isSaving)
                Button(isSaving ? "Saving…" : "Save") { submit() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(isSaving || !draft.canSave)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.profileSheet)
        // The sheet opens at the height its cards need.
        .presentationSizing(.fitted)
        .interactiveDismissDisabled(isSaving)
        .task {
            for provider in Set(draft.runners.map(\.provider)) { await loadModels(provider) }
        }
    }

    /// The cards' height, at least one card and at most runnerListMax, where the list scrolls.
    /// Before the list lays out, each card counts as runnerCard.
    private var listHeight: CGFloat {
        let cards = contentHeight ?? CGFloat(draft.runners.count) * DesignTokens.Size.runnerCard
        return min(max(cards, DesignTokens.Size.runnerCard), DesignTokens.Size.runnerListMax)
    }

    private func provider(_ id: String) -> SwarmProvider? {
        providers.first { $0.id == id }
    }

    private func move(_ index: Int, by offset: Int) {
        let target = index + offset
        guard draft.runners.indices.contains(target) else { return }
        draft.move(fromOffsets: IndexSet(integer: index), toOffset: offset > 0 ? target + 1 : target)
    }

    private func changeProvider(_ choice: SwarmProvider, at index: Int) async {
        guard draft.runners.indices.contains(index) else { return }
        let id = draft.runners[index].id
        draft.setProvider(choice, at: index, firstModel: models[choice.id]?.first?.id)
        await loadModels(choice.id)
        fillEmptyModel(of: id)
    }

    /// A runner added or switched before its provider's list loaded takes the first listed model.
    private func fillEmptyModel(of id: UUID) {
        guard let index = draft.runners.firstIndex(where: { $0.id == id }),
              draft.runners[index].model.isEmpty,
              let first = models[draft.runners[index].provider]?.first?.id else { return }
        draft.runners[index].model = first
    }

    private func loadModels(_ provider: String, retry: Bool = false) async {
        if !retry, models[provider] != nil { return }
        do {
            models[provider] = try await SwarmModelCatalog.shared.models(for: provider)
            catalogErrors[provider] = nil
        } catch {
            catalogErrors[provider] = (error as? SwarmProfileError)?.message ?? String(describing: error)
        }
    }

    private func submit() {
        guard !isSaving else { return }
        isSaving = true
        error = nil
        let profile = draft.profile
        Task {
            defer { isSaving = false }
            do {
                try await save(profile)
                onSaved()
                dismiss()
            } catch {
                self.error = (error as? SwarmProfileError)?.message ?? String(describing: error)
            }
        }
    }
}

private struct RunnerCard: View {
    let index: Int
    @Binding var runner: SwarmRunner
    let provider: SwarmProvider?
    let providers: [SwarmProvider]
    let models: [SwarmModel]
    let catalogError: String?
    let status: RunnerCardStatus
    /// No runner of the saved profile can run.
    let blocked: Bool
    let canRemove: Bool
    let isLast: Bool
    let onProvider: (SwarmProvider) -> Void
    let onRemove: () -> Void
    let onMove: (Int) -> Void
    let onRetry: () -> Void
    @State private var showsFlags = false

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack(spacing: DesignTokens.Spacing.s) {
                Image(systemName: "line.3.horizontal").foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
                Text(index == 0 ? "\(index + 1)  PRIMARY" : "\(index + 1)  FALLBACK")
                    .font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                statusLabel
                Menu {
                    Button("Move Up") { onMove(-1) }.disabled(index == 0)
                    Button("Move Down") { onMove(1) }.disabled(isLast)
                    Divider()
                    Button("Remove", role: .destructive, action: onRemove).disabled(!canRemove)
                } label: {
                    Image(systemName: "ellipsis")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .accessibilityLabel("Runner \(index + 1) actions")
            }
            HStack(spacing: DesignTokens.Spacing.s) {
                Picker("Provider", selection: Binding(
                    get: { runner.provider },
                    set: { id in if let choice = providers.first(where: { $0.id == id }) { onProvider(choice) } }
                )) {
                    ForEach(providers) { Text($0.label).tag($0.id) }
                    if provider == nil { Text(runner.provider).tag(runner.provider) }
                }
                .labelsHidden()
                .fixedSize()
                ModelField(model: $runner.model, models: models)
                    .frame(width: DesignTokens.Size.modelField, alignment: .leading)
                Picker("Effort", selection: $runner.effort) {
                    ForEach(ProfileDraft.efforts(for: runner, provider: provider, models: models), id: \.self) {
                        Text($0).tag($0)
                    }
                }
                .labelsHidden()
                .fixedSize()
                Spacer(minLength: 0)
            }
            if let fields = provider?.fields, !fields.isEmpty {
                Button { showsFlags.toggle() } label: {
                    HStack(spacing: DesignTokens.Spacing.xs) {
                        Text(fields.map { "\($0.label): \(runner[field: $0.name] ?? "CLI default")" }
                            .joined(separator: " · "))
                        Image(systemName: "chevron.right").font(.caption2)
                    }
                    .font(.callout).foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityHint("Opens the flag pickers")
                .popover(isPresented: $showsFlags, arrowEdge: .bottom) {
                    FlagPickers(runner: $runner, fields: fields)
                }
            }
            ForEach(ProfileDraft.warnings(for: runner, provider: provider, models: models), id: \.self) {
                Label($0, systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.orange)
            }
            if let catalogError {
                HStack {
                    Text("Could not load models. Type a model name. \(catalogError)")
                        .font(.caption).foregroundStyle(.secondary)
                    Button("Retry", action: onRetry).controlSize(.small)
                }
            }
        }
        .padding(DesignTokens.Spacing.m)
        .background(fill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.card))
        .overlay {
            RoundedRectangle(cornerRadius: DesignTokens.Radius.card)
                .strokeBorder(status == .next ? Color.accentColor : DesignTokens.quoteBar,
                              lineWidth: DesignTokens.Size.hairline)
        }
        // One container element, so the Move actions reach VoiceOver and its children stay usable.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(index == 0 ? "Runner \(index + 1), primary" : "Runner \(index + 1), fallback")
        .accessibilityAction(named: "Move Up") { onMove(-1) }
        .accessibilityAction(named: "Move Down") { onMove(1) }
    }

    private var fill: Color {
        guard case .skipped = status else { return DesignTokens.codeBlockFill }
        return blocked ? DesignTokens.errorFill : DesignTokens.warningFill
    }

    @ViewBuilder
    private var statusLabel: some View {
        switch status {
        case .unknown:
            EmptyView()
        case .next:
            Label("Next launch", systemImage: "circle.fill")
                .font(.caption).foregroundStyle(Color.accentColor)
        case .standby:
            Text("Standby").font(.caption).foregroundStyle(.secondary)
        case .skipped(let reason):
            Label("Skipped: \(reason)", systemImage: "exclamationmark.triangle.fill")
                .font(.caption).foregroundStyle(.orange).lineLimit(1)
                .help(reason)
        case .added:
            Text("New").font(.caption.weight(.semibold)).foregroundStyle(Color.accentColor)
        case .changed:
            Text("Changed").font(.caption.weight(.semibold)).foregroundStyle(Color.accentColor)
        case .saveToUpdate:
            Text("Save to update").font(.caption).foregroundStyle(.tertiary)
        }
    }
}

/// One model control: a menu of the CLI's models with "Other model…", or a text field when the
/// list did not load or the model is not in it.
private struct ModelField: View {
    @Binding var model: String
    let models: [SwarmModel]
    @State private var typesOther = false

    var body: some View {
        if models.isEmpty || typesOther || !models.contains(where: { $0.id == model }) {
            HStack(spacing: DesignTokens.Spacing.xs) {
                TextField("Model", text: $model)
                    .textFieldStyle(.roundedBorder)
                    .font(DesignTokens.mono)
                if !models.isEmpty {
                    Menu {
                        list
                    } label: {
                        Image(systemName: "list.bullet").accessibilityLabel("Choose a listed model")
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                    .accessibilityLabel("Choose a listed model")
                }
            }
        } else {
            Menu {
                list
                Divider()
                Button("Other model…") { typesOther = true }
            } label: {
                Text(models.first { $0.id == model }?.label ?? model)
            }
            .fixedSize()
            .help(model)
            .accessibilityLabel("Model")
            .accessibilityValue(model)
        }
    }

    @ViewBuilder
    private var list: some View {
        ForEach(models) { item in
            Button(item.label) {
                model = item.id
                typesOther = false
            }
        }
    }
}

/// The flags a provider takes. "" is no flag at all, so the CLI uses its own default.
private struct FlagPickers: View {
    @Binding var runner: SwarmRunner
    let fields: [SwarmProviderField]

    var body: some View {
        Form {
            ForEach(fields, id: \.name) { field in
                Picker(field.label, selection: Binding(
                    get: { runner[field: field.name] ?? "" },
                    set: { runner[field: field.name] = $0.isEmpty ? nil : $0 }
                )) {
                    Text("CLI default").tag("")
                    ForEach(values(field), id: \.self) { Text($0).tag($0) }
                }
            }
        }
        .padding(DesignTokens.Spacing.m)
        .fixedSize()
    }

    /// The field's listed values, plus the runner's own value when the list lacks it, so the
    /// picker never drops a value the CLI accepted before its help changed.
    private func values(_ field: SwarmProviderField) -> [String] {
        guard let current = runner[field: field.name], !field.values.contains(current) else {
            return field.values
        }
        return field.values + [current]
    }
}
