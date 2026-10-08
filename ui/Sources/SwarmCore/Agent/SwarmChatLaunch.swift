import Foundation

/// What New Chat starts. With no provider and model, swarm starts the named profile's first runner
/// that can run; with both, it starts exactly that runner, once, with no fallback (ADR 0033).
public struct SwarmChatLaunchPlan: Sendable, Equatable {
    public let directory: String
    public let provider: String?
    public let role: String
    public let model: String?
    public let account: String?

    public static func validModel(_ name: String) -> Bool {
        !name.isEmpty && !name.hasPrefix("-") && !name.unicodeScalars.contains {
            CharacterSet.whitespacesAndNewlines.contains($0) || CharacterSet.controlCharacters.contains($0)
        }
    }

    /// A one-off pick of one provider and model. swarm refuses a provider it does not know and
    /// ignores the account for a provider with no accounts.
    public init?(
        directory: String, provider: String, model: String, account: SwarmAccountSelection?
    ) {
        let name = model.trimmingCharacters(in: .whitespacesAndNewlines)
        guard directory.hasPrefix("/"), !provider.isEmpty, Self.validModel(name) else {
            return nil
        }
        self.directory = directory
        self.provider = provider
        role = "chat"
        self.model = name
        self.account = switch account {
        case .auto: "auto"
        case .named(let name): name
        case nil: nil
        }
    }

    /// The chat profile. A named account belongs to one provider, so it needs a one-off pick; the
    /// profile takes only Auto, which swarm ignores for a provider with no accounts.
    public init?(profileIn directory: String) {
        self.init(role: "chat", in: directory)
    }

    public init?(profile: String, in directory: String) {
        self.init(role: profile, in: directory)
    }

    private init?(role: String, in directory: String) {
        guard !role.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              directory.hasPrefix("/") else { return nil }
        self.directory = directory
        provider = nil
        self.role = role
        model = nil
        account = "auto"
    }
}

public enum SwarmChatLauncher {
    /// Two `swarm init` at once on a new swarm home fail with "database is locked", so session
    /// creation runs one at a time. Each takes under 10 ms; launches still run side by side.
    private static let createGate = SerialGate()

    public static func start(_ plan: SwarmChatLaunchPlan, bus: any SwarmBus) async throws -> SwarmSessionID {
        let timing = SwarmPerformance.begin("ChatLaunch")
        defer { timing.end() }
        let id = try await create(plan, bus: bus)
        try await launch(plan, in: id, bus: bus)
        return id
    }

    public static func create(_ plan: SwarmChatLaunchPlan, bus: any SwarmBus) async throws -> SwarmSessionID {
        let timing = SwarmPerformance.begin("SessionCreate")
        defer { timing.end() }
        return try await createGate.run {
            try await bus.startChairSession(chair: nil, directory: plan.directory)
        }
    }

    @discardableResult
    public static func launch(
        _ plan: SwarmChatLaunchPlan, in id: SwarmSessionID, bus: any SwarmBus
    ) async throws -> SwarmLaunch {
        let timing = SwarmPerformance.begin("ProviderLaunch")
        defer { timing.end() }
        return try await bus.launch(
            SwarmPanePolicy.chair, role: plan.role, provider: plan.provider, model: plan.model,
            account: plan.account,
            in: id, directory: plan.directory
        )
    }

    public static func waitForChairPane(
        in session: SwarmSessionID, bus: any SwarmBus
    ) async throws -> SwarmAgent {
        for _ in 0..<20 {
            if let agent = try await bus.agents(in: session, adapter: "tmux-solo")
                .first(where: { $0.id == SwarmPanePolicy.chair && $0.pane != nil }) {
                return agent
            }
            try await Task.sleep(for: .milliseconds(500))
        }
        throw SwarmProfileError.failed("The chair has no pane after 10 seconds")
    }
}

public extension SwarmProfileError {
    var message: String {
        switch self {
        case .unavailable(let message), .failed(let message): message
        }
    }

    /// Alerts read `localizedDescription`, which is otherwise "The operation couldn't be completed".
    var errorDescription: String? { message }
}
