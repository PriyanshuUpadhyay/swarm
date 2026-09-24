import Foundation

public struct SwarmChatLaunchPlan: Sendable, Equatable {
    public let directory: String
    public let provider: String
    public let role: String
    public let account: String?

    public init?(directory: String, role: SwarmRole, account: SwarmAccountSelection?) {
        guard directory.hasPrefix("/"), SwarmLaunchChoice.providers.contains(role.provider) else {
            return nil
        }
        self.directory = directory
        provider = role.provider
        self.role = role.id
        self.account = switch account {
        case .auto where role.provider != "agy": "auto"
        case .named(let name) where role.provider != "agy": name
        default: nil
        }
    }
}

public enum SwarmChatLauncher {
    public static func start(
        _ plan: SwarmChatLaunchPlan, bus: any SwarmBus,
        onCreated: @Sendable (SwarmSessionID) async -> Void = { _ in }
    ) async throws -> SwarmSessionID {
        let id = try await bus.startChairSession(chair: nil, directory: plan.directory)
        await onCreated(id)
        _ = try await bus.launch(
            SwarmPanePolicy.chair, role: plan.role, provider: plan.provider, account: plan.account,
            in: id, directory: plan.directory
        )
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
