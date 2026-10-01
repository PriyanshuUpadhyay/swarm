import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class SwitchModelModel {
    private let profiles = SwarmCLIProfileSource()
    private var choices: [String: String] = [:]
    private var optionsTask: Task<Void, Never>?
    private var modelsRetry: Task<Void, Never>?
    private(set) var choice = ModelSwitchChoice(currentProvider: nil, currentModel: nil)
    /// Nil until read; the picker then offers Claude and Codex with their plain names.
    var providers: [SwarmProvider]?

    var provider = "codex"
    var models: [SwarmModel] = []
    var selectedModel = ""
    var query = ""
    var customModel = ""
    var showCustomModel = false
    var modelCaption: String?
    var isLoadingModels = false
    var accountOptions: [SwarmAccountOption] = []
    /// Auto until the accounts come in, so Switch never waits for them.
    var accountSelection: SwarmAccountSelection? = .auto
    var accountCaption: String?
    var errorMessage: String?
    var isStarting = false
    var isCancelling = false
    var phase: ChatSwitchPhase?
    var operation: Task<Void, Never>?

    var canSwitch: Bool {
        choice.canSwitch(provider: provider, model: selectedModel, isSwitching: isStarting)
    }

    var offeredProviders: [String] { ModelSwitchChoice.offered(providers) }

    var visibleModels: [SwarmModel] {
        var result = models
        if !selectedModel.isEmpty, !result.contains(where: { $0.id == selectedModel }) {
            result.insert(SwarmModel(id: selectedModel, label: selectedModel), at: 0)
        }
        let search = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return result.filter { search.isEmpty || $0.label.localizedCaseInsensitiveContains(search)
            || $0.id.localizedCaseInsensitiveContains(search) }
    }

    var effortCaption: String {
        guard let effort = providers?.first(where: { $0.id == provider })?.defaultEffort else {
            return "Reasoning: provider default"
        }
        return "Reasoning: \(effort) (provider default)"
    }

    func label(_ provider: String) -> String {
        providers?.first { $0.id == provider }?.label ?? provider.capitalized
    }

    func load(currentProvider: String?, currentModel: String?) async {
        choice = ModelSwitchChoice(currentProvider: currentProvider, currentModel: currentModel)
        provider = choice.initialProvider(offered: offeredProviders)
        selectedModel = choice.initialModel(provider: provider, models: [])
        loadOptions()
        providers = try? await SwarmProfileCatalog.shared.providers()
    }

    func selectProvider(_ provider: String) {
        guard provider != self.provider else { return }
        choices[self.provider] = selectedModel
        self.provider = provider
        selectedModel = choices[provider] ?? choice.initialModel(provider: provider, models: [])
        models = []
        query = ""
        modelCaption = nil
        isLoadingModels = false
        errorMessage = nil
        showCustomModel = false
        accountOptions = []
        accountSelection = .auto
        accountCaption = nil
        loadOptions()
    }

    func selectModel(_ id: String) {
        selectedModel = id
        choices[provider] = id
        errorMessage = nil
    }

    /// Reads the models again. The accounts stay, so an account the owner picked is kept.
    func retry() {
        modelCaption = nil
        let requested = provider
        // Its own task: cancelling `optionsTask` would also drop an account read still running.
        modelsRetry?.cancel()
        modelsRetry = Task { await loadModels(provider: requested) }
    }

    /// Shows the cached models at once and reads them only when none are cached. Accounts read
    /// beside it. A provider change cancels both, so a late answer cannot land on the new provider.
    private func loadOptions() {
        optionsTask?.cancel()
        modelsRetry?.cancel()
        let requested = provider
        optionsTask = Task {
            async let modelsRead: Void = loadModels(provider: requested)
            let accounts = await loadAccounts(provider: requested)
            if !Task.isCancelled {
                accountOptions = accounts.options
                accountSelection = accounts.selection
                accountCaption = accounts.fallbackCaption
            }
            await modelsRead
        }
    }

    private func loadModels(provider: String) async {
        if let cached = await SwarmModelCatalog.shared.cached(provider), !cached.isEmpty {
            if !Task.isCancelled { show(cached) }
            return
        }
        guard !Task.isCancelled else { return }
        // With no list to pick from, the owner can still type a model name.
        isLoadingModels = true
        showCustomModel = true
        do {
            let fresh = try await SwarmModelCatalog.shared.models(for: provider)
            guard !Task.isCancelled else { return }
            show(fresh)
        } catch {
            guard !Task.isCancelled else { return }
            modelCaption = "Could not load models. Retry or enter a model name. \(message(error))"
        }
        isLoadingModels = false
    }

    private func show(_ list: [SwarmModel]) {
        models = list
        selectedModel = ChatModelChoice.initial(current: selectedModel, models: list)
    }

    private func loadAccounts(provider: String) async -> SwarmAccountLoadDecision {
        do { return .loaded(try await profiles.accounts(provider: provider)) }
        catch { return .failed(message: message(error)) }
    }

    func start(
        directory: String,
        launch: (SwarmChatLaunchPlan, @escaping @Sendable (ChatSwitchPhase) async -> Void) async throws -> SwarmSessionID
    ) async -> SwarmSessionID? {
        guard canSwitch, let plan = SwarmChatLaunchPlan(
            directory: directory, provider: provider, model: selectedModel, account: accountSelection
        ) else { return nil }
        isStarting = true
        phase = .preparing
        errorMessage = nil
        defer { isStarting = false; isCancelling = false; phase = nil }
        do {
            let id = try await launch(plan) { phase in
                await MainActor.run { self.phase = phase }
            }
            return id
        } catch is CancellationError {
            errorMessage = "Switch cancelled. The current chat stays selected."
            return nil
        } catch {
            errorMessage = message(error)
            return nil
        }
    }

    func cancel() {
        optionsTask?.cancel()
        modelsRetry?.cancel()
        guard phase?.canCancel == true else { return }
        isCancelling = true
        operation?.cancel()
    }

    private func message(_ error: any Error) -> String {
        (error as? SwarmProfileError)?.message ?? String(describing: error)
    }
}

struct SwitchModelSheet: View {
    let directory: String
    let currentProvider: String?
    let currentModel: String?
    let launch: (SwarmChatLaunchPlan, @escaping @Sendable (ChatSwitchPhase) async -> Void) async throws -> SwarmSessionID
    let onSwitched: (SwarmSessionID) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var model = SwitchModelModel()
    @State private var showAccount = false

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            Text("Switch model").font(.title2.bold())
            HStack(spacing: DesignTokens.Spacing.s) {
                Text("Now").foregroundStyle(.secondary)
                Text(verbatim: "\(currentProvider.map(model.label) ?? "Provider not reported") · "
                    + (currentModel ?? "Model not reported"))
            }
            selection.disabled(model.isStarting)
            Label("The new agent gets a summary or recent messages, not the full conversation.",
                  systemImage: "info.circle")
                .font(.callout).foregroundStyle(.secondary)
            if let phase = model.phase {
                HStack {
                    DelayedProgress()
                    Text(model.isCancelling ? "Cancelling switch…" : phase.title)
                }
                Text(phase.canCancel
                     ? "You can cancel before the new agent starts. The current agent may finish its summary."
                     : "The new agent is starting. Keep this window open until the switch finishes.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let error = model.errorMessage {
                Text(verbatim: error).font(.callout).foregroundStyle(.red).textSelection(.enabled)
            }
            HStack {
                Spacer()
                Button(model.isStarting ? "Cancel switch" : "Cancel") {
                    if model.isStarting { model.cancel() } else { dismiss() }
                }
                .keyboardShortcut(.cancelAction)
                .disabled(model.isStarting && (model.phase?.canCancel != true || model.isCancelling))
                Button("Switch") {
                    guard model.operation == nil else { return }
                    model.operation = Task {
                        defer { model.operation = nil }
                        if let id = await model.start(directory: directory, launch: launch) {
                            onSwitched(id)
                            dismiss()
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!model.canSwitch)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet)
        .interactiveDismissDisabled(model.isStarting)
        .task { await model.load(currentProvider: currentProvider, currentModel: currentModel) }
        .onDisappear { model.cancel() }
    }

    private var selection: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            Picker("Provider", selection: Binding(
                get: { model.provider }, set: { model.selectProvider($0) }
            )) {
                ForEach(model.offeredProviders, id: \.self) { provider in
                    Text(model.label(provider)).tag(provider)
                }
            }
            .pickerStyle(.segmented)
            TextField("Search models", text: $model.query)
                .textFieldStyle(.roundedBorder)
                .accessibilityLabel("Search models")
            if model.isLoadingModels && model.models.isEmpty {
                DelayedProgress("Loading models…")
                    .frame(height: DesignTokens.Size.pickerList)
            } else {
                modelList
            }
            if let caption = model.modelCaption {
                Text(verbatim: caption).font(.caption).foregroundStyle(.secondary)
                Button("Retry") { model.retry() }
            }
            DisclosureGroup("Other model", isExpanded: $model.showCustomModel) {
                HStack {
                    TextField("Model name", text: $model.customModel)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit(useCustomModel)
                    Button("Use", action: useCustomModel)
                        .disabled(!SwarmChatLaunchPlan.validModel(customModelName))
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
                DisclosureGroup(accountLabel, isExpanded: $showAccount) {
                    if !model.accountOptions.isEmpty {
                        Picker("Account", selection: $model.accountSelection) {
                            ForEach(model.accountOptions) { option in
                                Text(option.label).tag(option.selection as SwarmAccountSelection?)
                                    .disabled(option.disabledReason != nil)
                            }
                        }
                    } else {
                        Text("Use the provider's default account")
                    }
                    if let caption = model.accountCaption {
                        Text(verbatim: caption).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Text(model.effortCaption).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var customModelName: String {
        model.customModel.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Return in the Model name field picks the name, as Use does; it does not start the switch.
    private func useCustomModel() {
        guard SwarmChatLaunchPlan.validModel(customModelName) else { return }
        model.selectModel(customModelName)
        model.query = ""
    }

    private var accountLabel: String {
        let selected = model.accountOptions.first { $0.selection == model.accountSelection }
        return "Account · " + (selected?.label ?? (model.accountSelection == .auto ? "Auto" : "Provider default"))
    }

    private var modelList: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                if model.visibleModels.isEmpty {
                    Text("No matching models").foregroundStyle(.secondary).padding(DesignTokens.Spacing.s)
                }
                ForEach(model.visibleModels) { choice in
                    Button {
                        model.selectModel(choice.id)
                    } label: {
                        HStack {
                            Text(choice.label)
                            Spacer()
                            if choice.id == model.selectedModel { Image(systemName: "checkmark") }
                        }
                        .padding(DesignTokens.Spacing.s)
                        .contentShape(Rectangle())
                        .background(choice.id == model.selectedModel ? DesignTokens.selectionAccentFill : .clear,
                                    in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
                    }
                    .buttonStyle(.plain)
                    .help(choice.id)
                    .accessibilityAddTraits(choice.id == model.selectedModel ? .isSelected : [])
                }
            }
        }
        .frame(height: DesignTokens.Size.pickerList)
    }
}
