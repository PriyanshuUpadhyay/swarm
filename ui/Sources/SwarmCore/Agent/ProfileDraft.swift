import Foundation

/// A profile being edited. It keeps the rules the editor sheet shows (at least one runner, a
/// provider change resets the model and flags, warnings) out of the view.
public struct ProfileDraft: Sendable, Equatable {
    public let original: SwarmProfile
    public var runners: [SwarmRunner]

    public init(_ profile: SwarmProfile) {
        original = profile
        runners = profile.runners
    }

    public var profile: SwarmProfile { SwarmProfile(name: original.name, runners: runners) }

    public var canSave: Bool {
        runners != original.runners && !runners.isEmpty
            && runners.allSatisfy { SwarmChatLaunchPlan.validModel($0.model) }
    }

    public var canRemove: Bool { runners.count > 1 }

    /// Adds a runner of the first provider the profile does not use yet, else the first provider.
    public mutating func add(from providers: [SwarmProvider], firstModel: (String) -> String?) {
        let used = Set(runners.map(\.provider))
        guard let provider = providers.first(where: { !used.contains($0.id) }) ?? providers.first else {
            return
        }
        runners.append(provider.runner(model: firstModel(provider.id) ?? ""))
    }

    public mutating func remove(at index: Int) {
        guard canRemove, runners.indices.contains(index) else { return }
        runners.remove(at: index)
    }

    /// Moves rows as a List's `onMove` reports them: `destination` counts the rows before the
    /// move, as SwiftUI's `move(fromOffsets:toOffset:)` does.
    public mutating func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        let moving = source.filter(runners.indices.contains).map { runners[$0] }
        var rest = runners.indices.filter { !source.contains($0) }.map { runners[$0] }
        let at = destination - source.filter { $0 < destination }.count
        rest.insert(contentsOf: moving, at: min(max(at, 0), rest.count))
        runners = rest
    }

    /// A new provider takes its own model, default effort, and default flags: the old values
    /// mean nothing to it.
    public mutating func setProvider(_ provider: SwarmProvider, at index: Int, firstModel: String?) {
        guard runners.indices.contains(index), runners[index].provider != provider.id else { return }
        var runner = provider.runner(model: firstModel ?? "")
        runner.id = runners[index].id
        runners[index] = runner
    }

    /// The efforts a runner can take: its model's own list when the CLI gave one, else the
    /// provider's. The current value stays in the list so a picker never drops it.
    public static func efforts(
        for runner: SwarmRunner, provider: SwarmProvider?, models: [SwarmModel]
    ) -> [String] {
        let listed = models.first { $0.id == runner.model }?.efforts ?? provider?.efforts ?? []
        return listed.contains(runner.effort) ? listed : listed + [runner.effort]
    }

    /// What the editor warns about for one runner. Each is saved as typed, because the CLIs'
    /// lists change between releases.
    public static func warnings(
        for runner: SwarmRunner, provider: SwarmProvider?, models: [SwarmModel]
    ) -> [String] {
        var warnings: [String] = []
        if !models.isEmpty, !runner.model.isEmpty, !models.contains(where: { $0.id == runner.model }) {
            warnings.append("Model \"\(runner.model)\" is not in \(provider?.label ?? runner.provider)'s list. It is saved as typed.")
        }
        for field in provider?.fields ?? [] {
            if let value = runner[field: field.name], !field.values.contains(value) {
                warnings.append("\(field.label) \"\(value)\" is not in the list the CLI showed. It is saved as typed.")
            }
        }
        return warnings
    }
}

/// The status line under a profile row: which runner the next launch takes and why earlier ones
/// were skipped.
public struct ProfileStatus: Sendable, Equatable {
    public enum Kind: Sendable, Equatable { case primary, fallback, none }

    public let kind: Kind
    public let text: String

    public init(kind: Kind, text: String) {
        self.kind = kind
        self.text = text
    }

    public init?(check: SwarmProfileCheck?, profile: SwarmProfile) {
        guard let check else { return nil }
        let name = { (index: Int) in
            profile.runners.indices.contains(index) ? profile.runners[index].provider : "runner \(index + 1)"
        }
        let skips = check.skipped.map { "\(name($0.index)) skipped: \($0.text)" }
        switch check.pick {
        case .some(let pick) where check.skipped.isEmpty:
            self.init(kind: .primary, text: "Next launch: \(name(pick))")
        case .some(let pick):
            self.init(
                kind: .fallback,
                text: (["Next launch: \(name(pick))"] + skips).joined(separator: ". ")
            )
        case .none:
            self.init(
                kind: .none,
                text: "No runner can run. " + check.skipped
                    .map { "\(name($0.index)): \($0.text)" }.joined(separator: "; ")
            )
        }
    }
}

/// How New Chat relates its current pick to the chat profile (ADR 0032).
public struct ChatProfileChoice: Sendable, Equatable {
    /// The runner the chat profile would start now.
    public let runner: SwarmRunner
    /// The providers after it, in order.
    public let fallbacks: [String]
    public let profile: SwarmProfile

    public init?(profile: SwarmProfile?, check: SwarmProfileCheck?) {
        guard let profile, let first = profile.runners.first else { return nil }
        let pick = check?.pick.flatMap { profile.runners.indices.contains($0) ? $0 : nil } ?? 0
        self.profile = profile
        runner = pick == 0 ? first : profile.runners[pick]
        fallbacks = profile.runners.enumerated()
            .filter { $0.offset > pick }.map(\.element.provider)
    }

    public func isProfilePick(provider: String, model: String) -> Bool {
        provider == runner.provider && model == runner.model
    }

    /// The line under the pickers. A one-off pick takes the chat profile's effort for that
    /// provider, else the provider's default, as `swarm launch` does.
    public func caption(provider: String, model: String, defaultEffort: String?) -> String {
        if isProfilePick(provider: provider, model: model) {
            let rest = fallbacks.isEmpty ? "no fallback" : "falls back to " + fallbacks.joined(separator: ", ")
            return "From the chat profile · \(runner.effort) effort · \(rest)"
        }
        let effort = profile.runners.first { $0.provider == provider }?.effort ?? defaultEffort ?? "medium"
        return "One-off pick · \(effort) effort · no fallback"
    }
}
