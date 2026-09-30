import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class NewChatModel {
    private let profiles = SwarmCLIProfileSource()
    private var choices: [String: String] = [:]
    private var generation = 0
    private var optionsTask: Task<Void, Never>?
    /// The chat profile and the runner it would start now (ADR 0032). Nil until read, and for a
    /// model switch, which starts from the current chat instead.
    var chatChoice: ChatProfileChoice?
    var profileList: SwarmProfileList?
    var providers: [SwarmProvider] = []

    var provider = "codex"
    var isSwitch = false
    var models: [SwarmModel] = []
    var selectedModel = ""
    var query = ""
    var customModel = ""
    var showCustomModel = false
    var modelCaption: String?
    var accountOptions: [SwarmAccountOption] = []
    var accountSelection: SwarmAccountSelection?
    var accountCaption: String?
    var errorMessage: String?
    var isLoading = true
    var isStarting = false
    var isCancelling = false
    var phase: ChatSwitchPhase?
    var operation: Task<Void, Never>?

    var canStart: Bool {
        SwarmChatLaunchPlan.validModel(selectedModel) && !isLoading && !isStarting
    }

    var visibleModels: [SwarmModel] {
        var result = models
        if !selectedModel.isEmpty, !result.contains(where: { $0.id == selectedModel }) {
            result.insert(SwarmModel(id: selectedModel, label: selectedModel), at: 0)
        }
        let search = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return result.filter { search.isEmpty || $0.label.localizedCaseInsensitiveContains(search)
            || $0.id.localizedCaseInsensitiveContains(search) }
    }

    func load(initialProvider: String?, initialModel: String?) async {
        let allowed = isSwitch ? ["claude", "codex"] : SwarmChatProvider.all
        if let preferred = initialProvider, allowed.contains(preferred) {
            provider = preferred
        }
        selectedModel = ChatModelChoice.initial(current: initialModel, models: [])
        await loadChatProfile(check: nil)
        models = await SwarmModelCatalog.shared.cached(provider) ?? []
        await loadOptions()
        if !isSwitch { await loadChatProfileCheck() }
    }

    /// Reads the chat profile and selects the runner it would start. The launch check is slower,
    /// so the first pass uses the profile's first runner.
    func loadChatProfile(check: SwarmProfileCheck?) async {
        guard let list = try? await profiles.profiles() else { return }
        profileList = list
        if providers.isEmpty { providers = (try? await profiles.providers()) ?? [] }
        let before = chatChoice
        chatChoice = ChatProfileChoice(profile: list.profiles.first { $0.name == "chat" }, check: check)
        guard !isSwitch, let runner = chatChoice?.runner,
              before == nil || before?.isProfilePick(provider: provider, model: selectedModel) == true,
              SwarmChatProvider.all.contains(runner.provider) else { return }
        if runner.provider != provider {
            selectProvider(runner.provider)
        }
        selectModel(runner.model)
    }

    /// Moves the pick to the runner the launch check chose, unless the owner already picked
    /// something else.
    private func loadChatProfileCheck() async {
        guard let check = try? await profiles.check().first(where: { $0.name == "chat" }) else { return }
        await loadChatProfile(check: check)
    }

    var usesChatProfile: Bool {
        // No selection is a provider with no accounts, or no yelo; swarm then uses the CLI login.
        !isSwitch && (accountSelection ?? .auto) == .auto
            && chatChoice?.isProfilePick(provider: provider, model: selectedModel) == true
    }

    var effortCaption: String {
        guard let chatChoice else { return "Reasoning effort: the provider's default" }
        let fallback = providers.first { $0.id == provider }?.defaultEffort
        // A switch or a named account is always a one-off; an empty model never matches the
        // profile's pick, so the caption says so.
        let model = usesChatProfile ? selectedModel : ""
        return chatChoice.caption(provider: provider, model: model, defaultEffort: fallback)
    }

    func selectProvider(_ provider: String) {
        guard provider != self.provider else { return }
        choices[self.provider] = selectedModel
        self.provider = provider
        selectedModel = choices[provider]
            ?? chatChoice?.profile.runners.first { $0.provider == provider }?.model ?? ""
        models = []
        query = ""
        modelCaption = nil
        errorMessage = nil
        showCustomModel = false
        accountOptions = []
        accountSelection = nil
        accountCaption = nil
        isLoading = true
        optionsTask?.cancel()
        optionsTask = Task { await loadOptions() }
    }

    func selectModel(_ id: String) {
        selectedModel = id
        choices[provider] = id
        errorMessage = nil
    }

    private func loadOptions() async {
        guard !Task.isCancelled else { return }
        generation += 1
        let requestedGeneration = generation
        let requested = provider
        async let modelResult = loadModels(provider: requested)
        async let accountResult = loadAccounts(provider: requested)
        let (catalog, accounts) = await (modelResult, accountResult)
        guard !Task.isCancelled, generation == requestedGeneration, provider == requested else { return }
        if catalog.caption == nil { models = catalog.models }
        modelCaption = catalog.caption
        selectedModel = ChatModelChoice.initial(current: selectedModel, models: models)
        accountOptions = accounts.options
        accountSelection = accounts.selection
        accountCaption = accounts.fallbackCaption
        isLoading = false
    }

    private func loadModels(provider: String) async -> (models: [SwarmModel], caption: String?) {
        do {
            return (try await SwarmModelCatalog.shared.models(for: provider), nil)
        } catch {
            return ([], "Could not load models. Retry or enter a model name. \(message(error))")
        }
    }

    private func loadAccounts(provider: String) async -> SwarmAccountLoadDecision {
        do { return .loaded(try await profiles.accounts(provider: provider)) }
        catch { return .failed(message: message(error)) }
    }

    func retry() async {
        isLoading = true
        await loadOptions()
    }

    func start(
        directory: String,
        launch: (SwarmChatLaunchPlan, @escaping @Sendable (ChatSwitchPhase) async -> Void) async throws -> SwarmSessionID
    ) async -> SwarmSessionID? {
        let plan = usesChatProfile
            ? SwarmChatLaunchPlan(profileIn: directory)
            : SwarmChatLaunchPlan(
                directory: directory, provider: provider, model: selectedModel, account: accountSelection
            )
        guard canStart, let plan else { return nil }
        isStarting = true
        phase = isSwitch ? .preparing : .starting
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
        guard phase?.canCancel == true else { return }
        isCancelling = true
        operation?.cancel()
    }

    private func message(_ error: any Error) -> String {
        (error as? SwarmProfileError)?.message ?? String(describing: error)
    }
}

struct NewChatSheet: View {
    let directory: String
    var isSwitch = false
    var initialProvider: String?
    var initialModel: String?
    let launch: (SwarmChatLaunchPlan, @escaping @Sendable (ChatSwitchPhase) async -> Void) async throws -> SwarmSessionID
    let onStarted: (SwarmSessionID) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var model = NewChatModel()
    @State private var showAccount = false
    @State private var editingProfile: SwarmProfile?

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            Text(isSwitch ? "Choose model" : "New chat").font(.title2.bold())
            if isSwitch {
                Text("Current: \(initialModel ?? "Model not reported")")
                    .foregroundStyle(.secondary)
                Text("This starts a new agent with a summary or recent messages. The full conversation is not sent.")
                    .font(.callout).foregroundStyle(.secondary)
            } else {
                Text(verbatim: directory).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    .truncationMode(.middle).help(directory)
            }
            selection.disabled(model.isStarting)
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
                Button(model.isStarting ? "Cancel switch" : "Cancel") {
                    if model.isStarting { model.cancel() } else { dismiss() }
                }
                .keyboardShortcut(.cancelAction)
                .disabled(model.isStarting && (model.phase?.canCancel != true || model.isCancelling))
                Spacer()
                Button(isSwitch ? "Switch model" : "Start chat") {
                    guard model.operation == nil else { return }
                    model.operation = Task {
                        defer { model.operation = nil }
                        if let id = await model.start(directory: directory, launch: launch) {
                            onStarted(id)
                            dismiss()
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!model.canStart || (isSwitch && model.provider == initialProvider
                    && model.selectedModel == initialModel))
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet)
        .interactiveDismissDisabled(model.isStarting)
        .task {
            model.isSwitch = isSwitch
            await model.load(initialProvider: initialProvider, initialModel: initialModel)
        }
        .onDisappear { model.cancel() }
        .sheet(item: $editingProfile) { chat in
            ProfileEditorSheet(
                profile: chat,
                providers: model.providers,
                save: { edited in
                    guard let revision = model.profileList?.revision else {
                        throw SwarmProfileError.failed("Profiles are not loaded yet")
                    }
                    _ = try await SwarmCLIProfileSource().save(edited, revision: revision)
                },
                onSaved: {
                    // The saved profile's first runner becomes the pick, as when the sheet opened.
                    model.chatChoice = nil
                    Task { await model.loadChatProfile(check: nil) }
                }
            )
        }
    }

    private var selection: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            Picker("Provider", selection: Binding(
                get: { model.provider }, set: { model.selectProvider($0) }
            )) {
                ForEach(isSwitch ? ["claude", "codex"] : SwarmChatProvider.all, id: \.self) { provider in
                    Text(provider == "agy" ? "Gemini" : provider.capitalized).tag(provider)
                }
            }
            .pickerStyle(.segmented)
            TextField("Search models", text: $model.query)
                .textFieldStyle(.roundedBorder)
                .accessibilityLabel("Search models")
            if model.isLoading && model.models.isEmpty {
                DelayedProgress("Loading models and accounts…")
                    .frame(height: DesignTokens.Size.pickerList)
            } else {
                modelList
                if model.isLoading {
                    DelayedProgress("Checking models and accounts…")
                }
            }
            if let caption = model.modelCaption {
                Text(verbatim: caption).font(.caption).foregroundStyle(.secondary)
                Button("Retry") { Task { await model.retry() } }
            }
            DisclosureGroup("Other model", isExpanded: $model.showCustomModel) {
                HStack {
                    TextField("Model name", text: $model.customModel)
                        .textFieldStyle(.roundedBorder)
                    Button("Use") {
                        model.selectModel(model.customModel.trimmingCharacters(in: .whitespacesAndNewlines))
                        model.query = ""
                    }
                    .disabled(!SwarmChatLaunchPlan.validModel(
                        model.customModel.trimmingCharacters(in: .whitespacesAndNewlines)
                    ))
                }
            }
            HStack {
                Text(model.effortCaption).font(.caption).foregroundStyle(.secondary)
                Spacer()
                if !isSwitch, let chat = model.profileList?.profiles.first(where: { $0.name == "chat" }) {
                    Button("Edit profile…") { editingProfile = chat }
                        .controlSize(.small)
                }
            }
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
        }
    }

    private var accountLabel: String {
        let selected = model.accountOptions.first { $0.selection == model.accountSelection }
        return "Account · " + (selected?.label ?? "Provider default")
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
