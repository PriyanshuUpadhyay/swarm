import SwiftUI
import SwarmCore

/// Edits one profile's runners: add, remove, reorder, and each runner's provider, model, effort,
/// and the flags its provider takes. Save writes the whole profile in one call; Cancel drops
/// every edit.
struct ProfileEditorSheet: View {
    let providers: [SwarmProvider]
    let save: (SwarmProfile) async throws -> Void
    let onSaved: () -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var draft: ProfileDraft
    @State private var models: [String: [SwarmModel]] = [:]
    @State private var catalogErrors: [String: String] = [:]
    @State private var isSaving = false
    @State private var error: String?

    init(
        profile: SwarmProfile, providers: [SwarmProvider],
        save: @escaping (SwarmProfile) async throws -> Void, onSaved: @escaping () -> Void
    ) {
        self.providers = providers
        self.save = save
        self.onSaved = onSaved
        // A one-time seed: the sheet owns the edits from here until Save or Cancel.
        _draft = State(initialValue: ProfileDraft(profile))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text("Edit profile · \(draft.original.name)").font(.title2)
            List {
                ForEach($draft.runners) { $runner in
                    let index = draft.runners.firstIndex { $0.id == runner.id } ?? 0
                    RunnerRow(
                        index: index,
                        runner: $runner,
                        provider: provider(runner.provider),
                        providers: providers,
                        models: models[runner.provider] ?? [],
                        catalogError: catalogErrors[runner.provider],
                        canRemove: draft.canRemove,
                        onProvider: { choice in
                            Task { await changeProvider(choice, at: index) }
                        },
                        onRemove: { draft.remove(at: index) },
                        onMove: { offset in
                            let target = index + offset
                            guard draft.runners.indices.contains(target) else { return }
                            draft.move(
                                fromOffsets: IndexSet(integer: index),
                                toOffset: offset > 0 ? target + 1 : target
                            )
                        },
                        onRetry: { Task { await loadModels(runner.provider, retry: true) } }
                    )
                }
                .onMove { draft.move(fromOffsets: $0, toOffset: $1) }
            }
            .frame(minHeight: DesignTokens.Size.sheetHeight)
            Button {
                draft.add(from: providers) { models[$0]?.first?.id }
                if let added = draft.runners.last { Task { await loadModels(added.provider) } }
            } label: {
                Label("Add runner", systemImage: "plus")
            }
            .disabled(providers.isEmpty)
            Text("The first runner that can run starts. A runner is skipped when its CLI is missing, no account is signed in, or usage is low.")
                .font(.caption).foregroundStyle(.secondary)
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
        .interactiveDismissDisabled(isSaving)
        .task {
            for provider in Set(draft.runners.map(\.provider)) { await loadModels(provider) }
        }
    }

    private func provider(_ id: String) -> SwarmProvider? {
        providers.first { $0.id == id }
    }

    private func changeProvider(_ choice: SwarmProvider, at index: Int) async {
        draft.setProvider(choice, at: index, firstModel: models[choice.id]?.first?.id)
        await loadModels(choice.id)
        if draft.runners.indices.contains(index), draft.runners[index].model.isEmpty,
           let first = models[choice.id]?.first?.id {
            draft.runners[index].model = first
        }
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

private struct RunnerRow: View {
    let index: Int
    @Binding var runner: SwarmRunner
    let provider: SwarmProvider?
    let providers: [SwarmProvider]
    let models: [SwarmModel]
    let catalogError: String?
    let canRemove: Bool
    let onProvider: (SwarmProvider) -> Void
    let onRemove: () -> Void
    let onMove: (Int) -> Void
    let onRetry: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack(spacing: DesignTokens.Spacing.s) {
                Image(systemName: "line.3.horizontal").foregroundStyle(.tertiary)
                    .frame(width: DesignTokens.Size.glyphSlot)
                    .accessibilityHidden(true)
                Text(index == 0 ? "\(index + 1)  Primary" : "\(index + 1)  Fallback")
                    .font(.caption).foregroundStyle(.secondary)
                    .frame(width: DesignTokens.Size.runnerLabel - DesignTokens.Size.glyphSlot - DesignTokens.Spacing.s, alignment: .leading)
                Picker("Provider", selection: Binding(
                    get: { runner.provider },
                    set: { id in if let choice = providers.first(where: { $0.id == id }) { onProvider(choice) } }
                )) {
                    ForEach(providers) { Text($0.label).tag($0.id) }
                    if provider == nil { Text(runner.provider).tag(runner.provider) }
                }
                .labelsHidden()
                .fixedSize()
                HStack(spacing: 0) {
                    TextField("Model", text: $runner.model)
                        .textFieldStyle(.roundedBorder)
                        .font(DesignTokens.mono)
                    Menu {
                        ForEach(models) { model in
                            Button(model.label) { runner.model = model.id }
                        }
                    } label: {
                        Image(systemName: "chevron.down")
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    .disabled(models.isEmpty)
                    .accessibilityLabel("Choose a listed model")
                }
                Picker("Effort", selection: $runner.effort) {
                    ForEach(ProfileDraft.efforts(for: runner, provider: provider, models: models), id: \.self) {
                        Text($0).tag($0)
                    }
                }
                .labelsHidden()
                .fixedSize()
                Button(action: onRemove) { Image(systemName: "minus.circle") }
                    .buttonStyle(.borderless)
                    .disabled(!canRemove)
                    .accessibilityLabel("Remove runner \(index + 1)")
            }
            if let fields = provider?.fields, !fields.isEmpty {
                HStack(spacing: DesignTokens.Spacing.l) {
                    ForEach(fields, id: \.name) { field in
                        // "" is no flag at all, so the CLI uses its own default.
                        Picker(field.label, selection: Binding(
                            get: { runner[field: field.name] ?? "" },
                            set: { runner[field: field.name] = $0.isEmpty ? nil : $0 }
                        )) {
                            Text("CLI default").tag("")
                            ForEach(values(field), id: \.self) { Text($0).tag($0) }
                        }
                        .fixedSize()
                    }
                }
                .padding(.leading, DesignTokens.Size.runnerLabel)
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
        .padding(.vertical, DesignTokens.Spacing.xs)
        .contextMenu {
            Button("Move Up") { onMove(-1) }.disabled(index == 0)
            Button("Move Down") { onMove(1) }
            Divider()
            Button("Remove", role: .destructive, action: onRemove).disabled(!canRemove)
        }
        .accessibilityAction(named: "Move Up") { onMove(-1) }
        .accessibilityAction(named: "Move Down") { onMove(1) }
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
