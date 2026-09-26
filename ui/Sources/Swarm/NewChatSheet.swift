import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class NewChatModel {
    private let profiles = SwarmCLIProfileSource()
    private let preferences = ChatModelPreferences()
    private var choices: [String: String] = [:]
    private var generation = 0

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
        if let preferred = initialProvider ?? preferences.provider, allowed.contains(preferred) {
            provider = preferred
        }
        selectedModel = ChatModelChoice.initial(
            current: initialModel, saved: preferences.model(for: provider), models: []
        )
        await loadOptions()
    }

    func selectProvider(_ provider: String) {
        guard provider != self.provider else { return }
        choices[self.provider] = selectedModel
        self.provider = provider
        selectedModel = choices[provider] ?? preferences.model(for: provider) ?? ""
        models = []
        query = ""
        modelCaption = nil
        errorMessage = nil
        showCustomModel = false
        accountOptions = []
        accountSelection = nil
        accountCaption = nil
        isLoading = true
        Task { await loadOptions() }
    }

    func selectModel(_ id: String) {
        selectedModel = id
        choices[provider] = id
        preferences.remember(provider: provider, model: id)
        errorMessage = nil
    }

    private func loadOptions() async {
        generation += 1
        let requestedGeneration = generation
        let requested = provider
        async let modelResult = loadModels(provider: requested)
        async let accountResult = loadAccounts(provider: requested)
        let (catalog, accounts) = await (modelResult, accountResult)
        guard generation == requestedGeneration, provider == requested else { return }
        models = catalog.models
        modelCaption = catalog.caption
        selectedModel = ChatModelChoice.initial(current: selectedModel, saved: nil, models: models)
        accountOptions = accounts.options
        accountSelection = accounts.selection
        accountCaption = accounts.fallbackCaption
        isLoading = false
    }

    private func loadModels(provider: String) async -> (models: [SwarmModel], caption: String?) {
        do {
            return (try await profiles.models(provider: provider), nil)
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
        guard canStart, let plan = SwarmChatLaunchPlan(
            directory: directory, provider: provider, model: selectedModel, account: accountSelection
        ) else { return nil }
        isStarting = true
        phase = isSwitch ? .preparing : .starting
        errorMessage = nil
        defer { isStarting = false; isCancelling = false; phase = nil }
        do {
            let id = try await launch(plan) { phase in
                await MainActor.run { self.phase = phase }
            }
            preferences.remember(provider: plan.provider, model: plan.model)
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

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
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
                    ProgressView().controlSize(.small)
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
        .padding(20)
        .frame(width: 460)
        .interactiveDismissDisabled(model.isStarting)
        .task {
            model.isSwitch = isSwitch
            await model.load(initialProvider: initialProvider, initialModel: initialModel)
        }
        .onDisappear { model.cancel() }
    }

    private var selection: some View {
        VStack(alignment: .leading, spacing: 12) {
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
            if model.isLoading {
                HStack { ProgressView().controlSize(.small); Text("Loading models and accounts…") }
                    .frame(height: 190)
            } else {
                modelList
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
            Text("Reasoning effort: Medium (fixed for new agents)")
                .font(.caption).foregroundStyle(.secondary)
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
            VStack(alignment: .leading, spacing: 2) {
                if model.visibleModels.isEmpty {
                    Text("No matching models").foregroundStyle(.secondary).padding(10)
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
                        .padding(9)
                        .contentShape(Rectangle())
                        .background(choice.id == model.selectedModel ? Color.accentColor.opacity(0.15) : .clear,
                                    in: RoundedRectangle(cornerRadius: 6))
                    }
                    .buttonStyle(.plain)
                    .help(choice.id)
                    .accessibilityAddTraits(choice.id == model.selectedModel ? .isSelected : [])
                }
            }
        }
        .frame(height: 190)
    }
}
