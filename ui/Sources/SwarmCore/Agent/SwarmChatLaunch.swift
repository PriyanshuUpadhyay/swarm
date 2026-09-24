import Foundation

public enum SwarmChatProvider {
    public static let all = ["claude", "codex", "agy"]
}

public struct SwarmChatLaunchPlan: Sendable, Equatable {
    public let directory: String
    public let provider: String
    public let role: String
    public let model: String
    public let account: String?

    public static func validModel(_ name: String) -> Bool {
        !name.isEmpty && !name.hasPrefix("-") && !name.unicodeScalars.contains {
            CharacterSet.whitespacesAndNewlines.contains($0) || CharacterSet.controlCharacters.contains($0)
        }
    }

    public init?(
        directory: String, provider: String, model: String, account: SwarmAccountSelection?
    ) {
        let name = model.trimmingCharacters(in: .whitespacesAndNewlines)
        guard directory.hasPrefix("/"), SwarmChatProvider.all.contains(provider),
              Self.validModel(name) else {
            return nil
        }
        self.directory = directory
        self.provider = provider
        role = "chat"
        self.model = name
        self.account = switch account {
        case .auto where provider != "agy": "auto"
        case .named(let name) where provider != "agy": name
        default: nil
        }
    }
}

public enum SwarmChatLauncher {
    public static func start(
        _ plan: SwarmChatLaunchPlan, bus: any SwarmBus,
        onCreated: @Sendable (SwarmSessionID) async -> Void = { _ in }
    ) async throws -> SwarmSessionID {
        let timing = SwarmPerformance.begin("ChatLaunch")
        defer { timing.end() }
        let id: SwarmSessionID
        do {
            let createTiming = SwarmPerformance.begin("SessionCreate")
            defer { createTiming.end() }
            id = try await bus.startChairSession(chair: nil, directory: plan.directory)
        }
        await onCreated(id)
        do {
            let launchTiming = SwarmPerformance.begin("ProviderLaunch")
            defer { launchTiming.end() }
            _ = try await bus.launch(
                SwarmPanePolicy.chair, role: plan.role, provider: plan.provider, model: plan.model,
                account: plan.account,
                in: id, directory: plan.directory
            )
        }
        return id
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
}
