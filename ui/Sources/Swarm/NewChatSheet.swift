import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class NewChatModel {
    private let profiles = SwarmCLIProfileSource()

    var provider = "claude"
    var roles: [SwarmRole] = []
    var roleID: String?
    var choice = SwarmLaunchChoice()
    var accountOptions: [SwarmAccountOption] = []
    var accountSelection: SwarmAccountSelection?
    var accountCaption: String?
    var errorMessage: String?
    var isLoading = true
    var isStarting = false

    var filteredRoles: [SwarmRole] { SwarmLaunchChoice.roles(roles, for: provider) }
    var selectedRole: SwarmRole? { filteredRoles.first { $0.id == roleID } }
    var canStart: Bool {
        selectedRole != nil && choice.roleID == selectedRole?.id && !isLoading && !isStarting
    }

    func load() async {
        do {
            roles = try await profiles.roles()
            selectProvider(provider)
            await loadAccounts()
        } catch {
            errorMessage = message(error)
        }
        isLoading = false
    }

    func selectProvider(_ provider: String) {
        self.provider = provider
        roleID = filteredRoles.first?.id
        _ = choice.selectRole(selectedRole)
        accountOptions = []
        accountSelection = nil
        accountCaption = nil
    }

    func selectRole(_ id: String?) {
        roleID = id
        _ = choice.selectRole(selectedRole)
    }

    func loadAccounts() async {
        let requested = provider
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
    }

    func start(
        directory: String,
        launch: (SwarmChatLaunchPlan) async throws -> SwarmSessionID
    ) async -> SwarmSessionID? {
        guard canStart, let role = selectedRole,
              let plan = SwarmChatLaunchPlan(
                directory: directory, role: role, account: accountSelection
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
    let launch: (SwarmChatLaunchPlan) async throws -> SwarmSessionID
    let onStarted: (SwarmSessionID) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var model = NewChatModel()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New chat").font(.title2)
            Text(verbatim: directory).foregroundStyle(.secondary)
            if model.isLoading {
                ProgressView("Loading roles")
            } else {
                Form {
                    Picker("Provider", selection: $model.provider) {
                        ForEach(SwarmLaunchChoice.providers, id: \.self) { provider in
                            Text(provider).tag(provider)
                        }
                    }
                    .onChange(of: model.provider) { _, provider in
                        model.selectProvider(provider)
                        Task { await model.loadAccounts() }
                    }
                    Picker("Role", selection: $model.roleID) {
                        ForEach(model.filteredRoles) { role in
                            Text(role.role).tag(role.id as String?)
                        }
                    }
                    .onChange(of: model.roleID) { _, id in model.selectRole(id) }
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
                Button(model.isStarting ? "Starting…" : "Start") {
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
        .frame(width: 440)
        .task { await model.load() }
    }
}
