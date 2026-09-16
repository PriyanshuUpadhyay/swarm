import Foundation

/// Which agent new sessions start on, asked on the welcome window's checks screen when this Mac
/// has more than one ready to run.
///
/// It was only in Settings, Models, as a model picker, and nothing on the way in said the choice
/// existed. The first person to walk through the sequence with both Claude Code and Codex signed
/// in found every session opening on Claude Code, because that is `AppDefaults.fallbackBackend`,
/// and wanted Codex. The checks screen has just listed both agents as ready, so that is where the
/// question is asked: not a screen of its own, and not asked at all on a Mac with one agent, where
/// there is nothing to choose between.
///
/// Choosing an agent is choosing a model, because a model id names its backend (see
/// `DefaultBackend`). So this picks that agent's own default model: Bloom's fallback for Claude
/// Code, whose list is written down, and the server's `isDefault` model for an agent whose list is
/// fetched. The effort carries over where the new model takes it.
public enum OnboardingAgentChoice {
    /// The agents the choice is between, in reading order: every runnable agent this report found
    /// installed and signed in. Empty until the agent rows have settled, and the choice is offered
    /// only when this holds two or more.
    public static func candidates(in report: SetupReport) -> [AgentKind] {
        let agents = SetupTool.displayOrder.filter { $0.agentKind?.canRunWorkspaces == true }
        guard agents.allSatisfy({ report.outcome(for: $0).isSettled }) else { return [] }
        return agents.filter { report.outcome(for: $0).isReady }.compactMap(\.agentKind)
    }

    public static func isOffered(in report: SetupReport) -> Bool {
        candidates(in: report).count > 1
    }

    /// The defaults after choosing `kind`, or nil when it cannot be answered yet: an agent whose
    /// model list is fetched has no model to name until the fetch comes back.
    ///
    /// Choosing the agent already in force changes nothing, so a model somebody picked in Settings
    /// is not replaced by that agent's default just because they confirmed it here.
    ///
    /// The review row follows only when it was following already, meaning it held the same model
    /// on the same backend. An owner who deliberately reviews on the other agent keeps that.
    public static func defaults(
        choosing kind: AgentKind,
        from current: AppDefaults,
        models: [AgentModel]
    ) -> AppDefaults? {
        guard kind != current.backend else { return current }

        let model: String
        let effort: String
        if kind == .claudeCode {
            // Bloom's own fallback effort rather than the one in force, which came from the other
            // agent and may be a level Claude Code does not take, such as Codex's `ultra`.
            model = AppDefaults.fallbackModel
            effort = AppDefaults.fallbackEffort
        } else {
            guard let chosen = AgentModel.selection(requested: nil, from: models) else { return nil }
            model = chosen.id
            effort = chosen.resolvedEffort(preferring: current.effort)
        }

        var next = current
        let reviewFollows = current.reviewBackend == current.backend && current.reviewModel == current.model
        next.model = model
        next.effort = effort
        next.backend = kind
        next.storedModel = model
        next.storedEffort = effort
        if reviewFollows {
            next.reviewModel = model
            next.reviewEffort = effort
            next.reviewBackend = kind
        }
        return next
    }
}
