import BloomCore
import Observation

/// The checks screen's "New sessions start on" choice: which agent is in force, and writing a new
/// one into the same settings rows Settings, Models edits.
///
/// Beside the view rather than in it, because choosing Codex can mean waiting for Codex's model
/// list, which is a fetch from its app server, and a view does not start a process. The rule for
/// what gets written is `OnboardingAgentChoice`, in the core with its tests.
@MainActor
@Observable
final class WelcomeAgentDefault {
    /// The agent new sessions start on, as far as this window knows. Nil until the settings have
    /// been read, which on a first launch waits for bootstrap to open the database.
    private(set) var selected: AgentKind?

    @ObservationIgnored private let store: () async -> Store?
    @ObservationIgnored private var writing: Task<Void, Never>?

    init(store: @escaping () async -> Store?) {
        self.store = store
    }

    func load() {
        guard selected == nil else { return }
        Task {
            guard let store = await store() else { return }
            let defaults = await AppDefaults.load(from: store)
            if selected == nil { selected = defaults.backend }
        }
    }

    /// Shown at once, written when it can be: a Codex default needs Codex's list, and a press
    /// that waited for the fetch before the control moved would read as a press that missed.
    func choose(_ kind: AgentKind) {
        selected = kind
        writing?.cancel()
        writing = Task {
            guard let store = await store() else { return }
            let catalog = ComposerModelCatalog.shared
            if kind != .claudeCode, (catalog.models[kind] ?? []).isEmpty {
                catalog.load()
                let deadline = ContinuousClock.now + .seconds(30)
                while (catalog.models[kind] ?? []).isEmpty, catalog.isLoading, ContinuousClock.now < deadline {
                    try? await Task.sleep(for: .milliseconds(200))
                    if Task.isCancelled { return }
                }
            }
            guard !Task.isCancelled else { return }
            let current = await AppDefaults.load(from: store)
            guard let next = OnboardingAgentChoice.defaults(
                choosing: kind, from: current, models: catalog.models[kind] ?? []
            ) else {
                // The list never came, so nothing was written. Showing the choice anyway would be
                // a control saying Codex over a Bloom that still opens Claude Code.
                selected = current.backend
                return
            }
            try? await next.saveChanges(from: current, to: store)
        }
    }
}
