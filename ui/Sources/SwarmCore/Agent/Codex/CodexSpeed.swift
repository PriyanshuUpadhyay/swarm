import Foundation

/// Codex's service tier is independent of Claude's thinking switch. Missing session state means
/// inherit Codex's merged configuration; an explicit false must survive as standard speed.
public struct CodexSpeed: Equatable, Sendable {
    public let isFast: Bool
    public let supportsFast: Bool

    public init(config: JSONValue, model: CodexModel) {
        supportsFast = config["features"]?["fast_mode"]?.boolValue != false && model.fastServiceTier != nil
        let tier = config["service_tier"]?.stringValue ?? model.defaultServiceTier
        isFast = supportsFast && (tier == "fast" || tier == "priority" || tier == model.fastServiceTier)
    }

    public func isFast(override: Bool?) -> Bool {
        supportsFast && (override ?? isFast)
    }

    public static func key(sessionID: SessionID) -> String { "session.\(sessionID).codexFastMode" }

    public static func override(stored: String?) -> Bool? {
        switch stored {
        case "1": true
        case "0": false
        default: nil
        }
    }

    public static func serviceTier(override: Bool?) -> String? {
        override.map { $0 ? "priority" : "default" }
    }

    /// Read through the server so profiles and project configuration use Codex's own precedence.
    /// No thread or model turn is started, and no configuration is written.
    public static func read(
        cwd: String,
        modelID: String,
        environment: [String: String] = Shell.environment()
    ) async throws -> CodexSpeed {
        try await read(cwd: cwd, modelID: modelID, environment: environment) {
            CodexClient(configuration: $0)
        }
    }

    static func read(
        cwd: String,
        modelID: String,
        environment: [String: String],
        makeClient: @Sendable (CodexClient.Configuration) -> CodexClient
    ) async throws -> CodexSpeed {
        let client = makeClient(.init(cwd: cwd, environment: environment))
        do {
            try await client.start()
            let config = try await client.readConfiguration(cwd: cwd)
            let models = try await client.listModels()
            let resolvedModel = ModelIdentifier.resolve(modelID).model
            guard let model = models.first(where: { $0.id == resolvedModel }) else {
                throw CodexClientError.unexpectedResult(method: "model/list")
            }
            let speed = CodexSpeed(config: config, model: model)
            await client.stop()
            return speed
        } catch {
            await client.stop()
            throw error
        }
    }
}
