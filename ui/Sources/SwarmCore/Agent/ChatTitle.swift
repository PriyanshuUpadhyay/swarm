import Foundation

public enum ChatTitle {
    public static func resolve(
        appName: String?, cliName: String?, firstLine: String?, provider: String?, id: SwarmSessionID
    ) -> String {
        if let name = nonblank(appName) { return name }
        if let name = nonblank(cliName) { return name }
        if let prompt = nonblank(firstLine) {
            return SwarmSessionTitle.make(sessionID: id, firstUserPrompt: prompt)
        }
        return "\(nonblank(provider) ?? "Chat") \(id.rawValue.prefix(8))"
    }

    /// The oldest link owns the app name even when a continuation changes the provider.
    public static func key(_ chat: SwarmProjectSession) -> String {
        chat.sessions.min {
            $0.createdAt == $1.createdAt ? $0.id.rawValue < $1.id.rawValue : $0.createdAt < $1.createdAt
        }!.id.rawValue
    }

    public static func title(_ chat: SwarmProjectSession, appName: String? = nil) -> String {
        resolve(appName: appName, cliName: chat.cliName, firstLine: chat.title, provider: chat.provider, id: chat.id)
    }

    private static func nonblank(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else { return nil }
        return value
    }
}
