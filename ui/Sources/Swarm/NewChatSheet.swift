import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class NewChatModel {
    private let profiles = SwarmCLIProfileSource()
    static let otherModel = "__other__"

    var provider = "codex"
    var isSwitch = false
    var models: [SwarmModel] = []
    var modelID = otherModel
    var customModel = ""
    var modelCaption: String?
    var isLoadingModels = false
    var accountOptions: [SwarmAccountOption] = []
    var accountSelection: SwarmAccountSelection?
    var accountCaption: String?
    var isLoadingAccounts = false
    var errorMessage: String?
    var isLoading = true
    var isStarting = false

    var selectedModel: String {
        modelID == Self.otherModel ? customModel.trimmingCharacters(in: .whitespacesAndNewlines) : modelID
    }
    var canStart: Bool {
        SwarmChatLaunchPlan.validModel(selectedModel)
            && !isLoading && !isLoadingModels && !isLoadingAccounts && !isStarting
    }

    func load(initialProvider: String?) async {
        let allowed = isSwitch ? ["claude", "codex"] : SwarmChatProvider.all
        if let initialProvider, allowed.contains(initialProvider) {
            provider = initialProvider
        }
        await loadModels()
        await loadAccounts()
        isLoading = false
    }

    func selectProvider(_ provider: String) {
        self.provider = provider
        models = []
        modelID = Self.otherModel
        customModel = ""
        modelCaption = nil
        clearAccounts()
        Task {
            await loadModels()
            await loadAccounts()
        }
    }

    func clearAccounts() {
        accountOptions = []
        accountSelection = nil
        accountCaption = nil
    }

    func loadModels() async {
        let requested = provider
        isLoadingModels = true
        do {
            let choices = try await profiles.models(provider: requested)
            guard provider == requested else { return }
            models = choices
            let preferred = [
                "claude": "sonnet", "codex": "gpt-6-sol", "agy": "gemini-3.8-flash-high"
            ][requested]
            modelID = choices.first { $0.id == preferred }?.id
                ?? choices.first?.id ?? Self.otherModel
            modelCaption = requested == "claude"
                ? "Claude lists aliases here. Choose Other model to enter a full model name."
                : nil
        } catch {
            guard provider == requested else { return }
            models = []
            modelID = Self.otherModel
            modelCaption = "Model list unavailable: \(message(error)). Enter a model name."
        }
        isLoadingModels = false
    }

    func loadAccounts() async {
        let requested = provider
        isLoadingAccounts = true
        do {
            let decision = SwarmAccountLoadDecision.loaded(
                try await profiles.accounts(provider: requested)
            )
            guard provider == requested else { return }
            accountOptions = decision.options
            accountSelection = decision.selection
            accountCaption = decision.fallbackCaption
        } catch {
            guard provider == requested else { return }
            let decision = SwarmAccountLoadDecision.failed(message: message(error))
            accountOptions = decision.options
            accountSelection = decision.selection
            accountCaption = decision.fallbackCaption
        }
        isLoadingAccounts = false
    }

    func start(
        directory: String,
        launch: (SwarmChatLaunchPlan) async throws -> SwarmSessionID
    ) async -> SwarmSessionID? {
        guard canStart, let plan = SwarmChatLaunchPlan(
            directory: directory, provider: provider, model: selectedModel,
            account: accountSelection
        ) else { return nil }
        isStarting = true
        errorMessage = nil
        do {
            let id = try await launch(plan)
            isStarting = false
            return id
        } catch {
            errorMessage = message(error)
            isStarting = false
            return nil
        }
    }

    private func message(_ error: any Error) -> String {
        (error as? SwarmProfileError)?.message ?? String(describing: error)
    }
}

struct NewChatSheet: View {
    let directory: String
    var isSwitch = false
    var initialProvider: String?
    let launch: (SwarmChatLaunchPlan) async throws -> SwarmSessionID
    let onStarted: (SwarmSessionID) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var model = NewChatModel()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(isSwitch ? "Switch model" : "New chat").font(.title2)
            Text(verbatim: directory).foregroundStyle(.secondary)
            if isSwitch {
                Text("Swarm will carry this chat's context to the new agent.")
                    .foregroundStyle(.secondary)
            }
            if model.isLoading {
                ProgressView("Loading models")
            } else {
                Form {
                    Picker("Provider", selection: Binding(
                        get: { model.provider }, set: { model.selectProvider($0) }
                    )) {
                        ForEach(isSwitch ? ["claude", "codex"] : SwarmChatProvider.all, id: \.self) { provider in
                            Text(provider.capitalized).tag(provider)
                        }
                    }
                    Picker("Model", selection: $model.modelID) {
                        ForEach(model.models) { choice in
                            Text(choice.label == choice.id ? choice.id : "\(choice.label) · \(choice.id)")
                                .tag(choice.id)
                        }
                        Text("Other model…").tag(NewChatModel.otherModel)
                    }
                    if model.modelID == NewChatModel.otherModel {
                        TextField("Model name", text: $model.customModel)
                    }
                    if model.isLoadingModels {
                        ProgressView("Loading models")
                    }
                    if let caption = model.modelCaption {
                        Text(verbatim: caption).foregroundStyle(.secondary)
                    }
                    if !model.accountOptions.isEmpty {
                        Picker("Account", selection: $model.accountSelection) {
                            ForEach(model.accountOptions) { option in
                                Text(option.label).tag(option.selection as SwarmAccountSelection?)
                                    .disabled(option.disabledReason != nil)
                            }
                        }
                    } else {
                        LabeledContent("Account", value: "Auto")
                    }
                    if let caption = model.accountCaption {
                        Text(verbatim: caption).foregroundStyle(.secondary)
                    }
                }
            }
            if let error = model.errorMessage {
                Text(verbatim: error).foregroundStyle(.red)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .accessibilityLabel("Cancel")
                Button(model.isStarting ? "Starting…" : (isSwitch ? "Switch" : "Start")) {
                    Task {
                        if let id = await model.start(directory: directory, launch: launch) {
                            onStarted(id)
                            dismiss()
                        }
                    }
                }
                .accessibilityLabel("Start")
                .disabled(!model.canStart)
            }
        }
        .padding(20)
        .frame(width: 500)
        .task {
            model.isSwitch = isSwitch
            await model.load(initialProvider: initialProvider)
        }
    }
}
