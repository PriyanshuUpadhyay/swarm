import Foundation

/// What Switch model offers first and when its action is ready (ADR 0035). The action never waits
/// for the model, account, or provider reads, so no rule here takes a loading flag.
public struct ModelSwitchChoice: Sendable, Equatable {
    /// A switch hands the conversation over through Claude's and Codex's hooks, so it offers only
    /// those two.
    public static let switchable = ["claude", "codex"]

    public let currentProvider: String?
    public let currentModel: String?

    public init(currentProvider: String?, currentModel: String?) {
        self.currentProvider = currentProvider
        self.currentModel = currentModel
    }

    public static func disabledReason(
        readOnlyReason: String?, waitingForModel: Bool, isSending: Bool, isRunning: Bool
    ) -> String? {
        if let readOnlyReason { return readOnlyReason }
        if waitingForModel { return "Waiting for this chat's model information." }
        if isSending || isRunning {
            return "Wait for the reply to finish, or stop it before switching model."
        }
        return nil
    }

    /// Background tabs have no transcript reader. Unknown state stays enabled so selecting the
    /// action can load the chat and check the composer's reason.
    public static func tabDisabledReason(
        readOnlyReason: String?, chat: SwarmProjectSession, agents: [SwarmAgent]
    ) -> String? {
        let chair = agents.first { SwarmPanePolicy.isChair($0, in: chat.session) }
        return disabledReason(readOnlyReason: readOnlyReason, waitingForModel: false,
                              isSending: false, isRunning: chair?.status.isMidTurn == true)
    }

    /// The providers the picker offers. Before the provider list is read, both switchable ones.
    public static func offered(_ providers: [SwarmProvider]?) -> [String] {
        guard let providers else { return switchable }
        return switchable.filter { id in providers.contains { $0.id == id } }
    }

    /// The chat's own provider when the picker offers it, else the first one offered.
    public func initialProvider(offered: [String]) -> String {
        currentProvider.flatMap { offered.contains($0) ? $0 : nil } ?? offered.first ?? "codex"
    }

    /// The chat's own model on its own provider, else the first listed model.
    public func initialModel(provider: String, models: [SwarmModel]) -> String {
        ChatModelChoice.initial(current: provider == currentProvider ? currentModel : nil, models: models)
    }

    public func canSwitch(provider: String, model: String, isSwitching: Bool) -> Bool {
        SwarmChatLaunchPlan.validModel(model)
            && !(provider == currentProvider && model == currentModel)
            && !isSwitching
    }
}
