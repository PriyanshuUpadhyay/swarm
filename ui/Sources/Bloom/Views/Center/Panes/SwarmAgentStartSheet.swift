import SwiftUI
import BloomCore

struct SwarmAgentStartSheet: View {
    @Bindable var model: WorkspaceModel

    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss

    @State private var roles: [SwarmRole] = []
    @State private var selectedRoleID: String?
    @State private var accountOptions: [SwarmAccountOption] = []
    @State private var selectedAccount: SwarmAccountSelection?
    @State private var accountCaption: String?
    @State private var name = ""
    @State private var suggestedName = ""
    @State private var firstMessage = ""
    @State private var errorMessage: String?
    @State private var isLoading = true
    @State private var isStarting = false

    private var selectedRole: SwarmRole? {
        roles.first { $0.id == selectedRoleID }
    }

    private var trimmedMessage: String {
        firstMessage.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canStart: Bool {
        selectedRole != nil
            && SwarmAgentName.isValid(name)
            && !takenNames.contains(SwarmAgentID(name))
            && !trimmedMessage.isEmpty
            && !isStarting
    }

    private var takenNames: Set<SwarmAgentID> {
        Set(CenterTabStore.shared.swarmAgents(in: model.workspace.id))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Metrics.spacingWide) {
            Text("Start a swarm agent")
                .font(Typo.title)

            if isLoading {
                ProgressView("Loading roles")
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Form {
                    Picker("Role", selection: $selectedRoleID) {
                        ForEach(roles) { role in
                            Text("\(role.role) (\(role.provider))").tag(role.id as String?)
                        }
                    }
                    .onChange(of: selectedRoleID) { _, _ in chooseRole() }

                    if selectedRole?.provider == "claude" || selectedRole?.provider == "codex" {
                        if !accountOptions.isEmpty {
                            Picker("Account", selection: $selectedAccount) {
                                ForEach(accountOptions) { option in
                                    Text(option.disabledReason.map { "\(option.label) (\($0))" }
                                         ?? option.label)
                                        .tag(option.selection as SwarmAccountSelection?)
                                        .disabled(option.disabledReason != nil)
                                }
                            }
                        }
                        if let accountCaption {
                            Text(accountCaption)
                                .font(Typo.caption)
                                .foregroundStyle(Palette.textTertiary)
                        }
                    }

                    TextField("Agent name", text: $name)
                    if !name.isEmpty, !SwarmAgentName.isValid(name) {
                        Text("Use 1 to 40 lowercase letters, numbers or hyphens. Start with a letter or number.")
                            .font(Typo.caption)
                            .foregroundStyle(Palette.negative)
                    } else if takenNames.contains(SwarmAgentID(name)) {
                        Text("That agent name is already open in this workspace.")
                            .font(Typo.caption)
                            .foregroundStyle(Palette.negative)
                    }

                    TextField("First message", text: $firstMessage, axis: .vertical)
                        .lineLimit(4...8)
                }
                .formStyle(.grouped)
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(Typo.caption)
                    .foregroundStyle(Palette.negative)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                Button(isStarting ? "Starting..." : "Start", action: start)
                    .keyboardShortcut(.return, modifiers: .command)
                    .disabled(!canStart)
            }
        }
        .padding(Metrics.spacingWide * 2)
        .frame(width: 500)
        .task { await loadRoles() }
        .task(id: selectedRoleID) { await loadAccounts() }
    }

    private func loadRoles() async {
        do {
            let loaded = try await app.swarmProfiles.roles()
                .filter { ["claude", "codex", "agy"].contains($0.provider) }
            guard !Task.isCancelled else { return }
            roles = loaded
            selectedRoleID = loaded.first?.id
            if loaded.isEmpty {
                errorMessage = "Swarm reported no Claude, Codex or AGY roles to start."
            }
        } catch {
            guard !Task.isCancelled else { return }
            errorMessage = errorText(error)
        }
        isLoading = false
        chooseRole()
    }

    private func chooseRole() {
        guard let role = selectedRole else { return }
        let proposed = SwarmAgentName.free(role: role.role, excluding: takenNames).rawValue
        if name.isEmpty || name == suggestedName { name = proposed }
        suggestedName = proposed
        accountOptions = []
        selectedAccount = nil
        accountCaption = nil
    }

    private func loadAccounts() async {
        guard let role = selectedRole,
              role.provider == "claude" || role.provider == "codex" else { return }
        do {
            let list = try await app.swarmProfiles.accounts(provider: role.provider)
            guard !Task.isCancelled, selectedRoleID == role.id else { return }
            apply(SwarmAccountLoadDecision.loaded(list))
        } catch let error as SwarmProfileError {
            guard !Task.isCancelled, selectedRoleID == role.id else { return }
            apply(SwarmAccountLoadDecision.failed(error))
        } catch {
            guard !Task.isCancelled, selectedRoleID == role.id else { return }
            apply(SwarmAccountLoadDecision.failed(message: error.localizedDescription))
        }
    }

    private func apply(_ decision: SwarmAccountLoadDecision) {
        accountOptions = decision.options
        selectedAccount = decision.selection
        accountCaption = decision.fallbackCaption
    }

    private func start() {
        guard canStart, let role = selectedRole else { return }
        isStarting = true
        errorMessage = nil
        let agent = SwarmAgentID(name)
        let account: String? = switch selectedAccount {
        case .auto: "auto"
        case .named(let name): name
        case nil: nil
        }
        Task {
            do {
                _ = try await model.swarmAgents.start(
                    agent: agent, role: role.role, account: account,
                    firstMessage: trimmedMessage
                )
                let tab = CenterTabStore.shared.add(
                    kind: .swarmAgent,
                    workspaceID: model.workspace.id,
                    title: agent.rawValue,
                    swarmAgent: agent
                )
                model.swarmAgents.sync(
                    agentIDs: CenterTabStore.shared.swarmAgents(in: model.workspace.id)
                )
                WorkspaceTabsStore.shared.select(.tool(tab.id), in: model)
                dismiss()
            } catch {
                errorMessage = errorText(error)
                isStarting = false
            }
        }
    }

    private func errorText(_ error: any Error) -> String {
        switch error {
        case SwarmProfileError.unavailable(let message), SwarmProfileError.failed(let message):
            message
        default:
            error.localizedDescription
        }
    }
}
