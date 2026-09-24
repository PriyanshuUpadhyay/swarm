import Observation
import SwiftUI
import SwarmCore

@MainActor @Observable
final class NewChatModel {
    private let profiles = SwarmCLIProfileSource()

    var provider = "all"
    var isSwitch = false
    var roles: [SwarmRole] = []
    var roleID: String?
    var choice = SwarmLaunchChoice()
    var accountOptions: [SwarmAccountOption] = []
    var accountSelection: SwarmAccountSelection?
    var accountCaption: String?
    var errorMessage: String?
    var isLoading = true
    var isStarting = false

    var filteredRoles: [SwarmRole] {
        SwarmLaunchChoice.roles(roles, for: provider, switching: isSwitch)
    }
    var selectedRole: SwarmRole? { filteredRoles.first { $0.launchID == roleID } }
    var canStart: Bool {
        selectedRole != nil && choice.roleID == selectedRole?.id && !isLoading && !isStarting
    }

    func load() async {
        do {
            roles = try await profiles.launchChoices()
            selectProvider(provider)
            await loadAccounts()
        } catch {
            errorMessage = message(error)
        }
        isLoading = false
    }

    func selectProvider(_ provider: String) {
        self.provider = provider
        roleID = filteredRoles.first?.launchID
        _ = choice.selectRole(selectedRole)
    }

    func clearAccounts() {
        accountOptions = []
        accountSelection = nil
        accountCaption = nil
    }

    func selectRole(_ id: String?) {
        roleID = id
        _ = choice.selectRole(selectedRole)
    }

    func loadAccounts() async {
        guard let requested = selectedRole?.provider else { return }
        do {
            let decision = SwarmAccountLoadDecision.loaded(
                try await profiles.accounts(provider: requested)
            )
            guard selectedRole?.provider == requested else { return }
            accountOptions = decision.options
            accountSelection = decision.selection
            accountCaption = decision.fallbackCaption
        } catch {
            guard selectedRole?.provider == requested else { return }
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
    var isSwitch = false
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
                ProgressView("Loading roles")
            } else {
                Form {
                    Picker("Provider", selection: $model.provider) {
                        ForEach(["all"] + (isSwitch ? ["claude", "codex"] : SwarmLaunchChoice.providers), id: \.self) { provider in
                            Text(provider == "all" ? "All" : provider.capitalized).tag(provider)
                        }
                    }
                    .onChange(of: model.provider) { _, provider in
                        model.selectProvider(provider)
                    }
                    Picker("Role", selection: $model.roleID) {
                        ForEach(model.filteredRoles, id: \.launchID) { role in
                            Text("\(role.role) · \(role.provider) · \(role.model)")
                                .tag(role.launchID as String?)
                        }
                    }
                    .onChange(of: model.roleID) { oldID, id in
                        model.selectRole(id)
                        let oldProvider = model.roles.first { $0.launchID == oldID }?.provider
                        if !model.isLoading, oldProvider != model.selectedRole?.provider {
                            model.clearAccounts()
                            Task { await model.loadAccounts() }
                        }
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
        .frame(width: 440)
        .task {
            model.isSwitch = isSwitch
            await model.load()
        }
    }
}
