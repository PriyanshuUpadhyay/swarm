import Foundation
import TranscriptTool

public enum InteractiveChatLifecycle {
    public enum State: String, Sendable, Hashable {
        case starting, running, stopped
    }

    public static func state(agentIsPresent: Bool, launchIsPending: Bool) -> State {
        if agentIsPresent { return .running }
        return launchIsPending ? .starting : .stopped
    }

    public static func resumeSessionID(_ stored: String?) -> String? {
        guard let id = stored?.trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty else { return nil }
        return id
    }

    public static let reportDeadline: Duration = .seconds(20)

    public static func isLate(
        launchedAt: Date, lastReportAt: Date?, now: Date = Date(),
        deadline: Duration = reportDeadline
    ) -> Bool {
        let seconds = TimeInterval(deadline.components.seconds)
            + TimeInterval(deadline.components.attoseconds) / 1e18
        guard now.timeIntervalSince(launchedAt) >= seconds else { return false }
        guard let lastReportAt else { return true }
        return lastReportAt < launchedAt
    }
}

/// Finds the provider-owned log for an interactive CLI and follows its typed events.
public enum InteractiveChatTranscript {
    public enum Failure: Error, Sendable, Hashable {
        case noSessionID, unsupported, missing, unreadable
    }

    public static func reader(
        agent: AgentKind, providerSessionID: String?,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> Result<any TranscriptReading, Failure> {
        guard let id = InteractiveChatLifecycle.resumeSessionID(providerSessionID) else {
            return .failure(.noSessionID)
        }
        guard let path = path(agent: agent, providerSessionID: id, home: home) else {
            return agent == .claudeCode || agent == .codex ? .failure(.missing) : .failure(.unsupported)
        }
        guard let binary = TranscriptToolProcess.bundled else { return .failure(.unreadable) }
        switch agent {
        case .claudeCode:
            return .success(ToolTranscriptReader(binary: binary, format: "claude", log: path))
        case .codex:
            return .success(ToolTranscriptReader(binary: binary, format: "codex", log: path))
        case .cursor, .openCode, .grok:
            return .failure(.unsupported)
        }
    }

    public static func conversationToResume(
        agent: AgentKind, providerSessionID: String?,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> String? {
        guard let id = InteractiveChatLifecycle.resumeSessionID(providerSessionID) else { return nil }
        if (agent == .claudeCode || agent == .codex)
            && path(agent: agent, providerSessionID: id, home: home) == nil { return nil }
        return id
    }

    public static func waitForReader(
        agent: AgentKind, providerSessionID: String?, sessionID: SwarmSessionID,
        home: URL = FileManager.default.homeDirectoryForCurrentUser,
        every interval: Duration = .seconds(1), status: URL? = nil
    ) async -> (any TranscriptReading)? {
        let status = status ?? AgentKind.interactiveStatusURL(sessionID: sessionID)
        while !Task.isCancelled {
            let reported = (try? Data(contentsOf: status))
                .flatMap { AgentKind.interactiveHookSessionID(data: $0) }
            switch reader(
                agent: agent,
                providerSessionID: InteractiveChatLifecycle.resumeSessionID(providerSessionID) ?? reported,
                home: home
            ) {
            case .success(let reader): return reader
            case .failure(.missing), .failure(.noSessionID):
                try? await Task.sleep(for: interval)
            case .failure(.unsupported), .failure(.unreadable): return nil
            }
        }
        return nil
    }

    public static func path(
        agent: AgentKind, providerSessionID: String,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL? {
        guard safe(providerSessionID) else { return nil }
        switch agent {
        case .claudeCode: return newest(claudePaths(providerSessionID, home: home))
        case .codex: return newest(codexPaths(providerSessionID, home: home))
        case .cursor, .openCode, .grok: return nil
        }
    }

    private static func safe(_ id: String) -> Bool {
        !id.isEmpty && id.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics.contains($0) || $0 == "-" || $0 == "_"
        }
    }

    private static func profileRoots(prefix: String, home: URL) -> [URL] {
        let manager = FileManager.default
        return (try? manager.contentsOfDirectory(
            at: home, includingPropertiesForKeys: nil
        ))?.filter { $0.lastPathComponent.hasPrefix(prefix) }
            .sorted { $0.path < $1.path } ?? []
    }

    private static func claudePaths(_ id: String, home: URL) -> [URL] {
        let manager = FileManager.default
        return profileRoots(prefix: ".claude", home: home).flatMap { root in
            let projects = root.appendingPathComponent("projects", isDirectory: true)
            let directories = (try? manager.contentsOfDirectory(
                at: projects, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
            )) ?? []
            return directories.compactMap { directory in
                let candidate = directory.appendingPathComponent(id + ".jsonl")
                return manager.fileExists(atPath: candidate.path) ? candidate : nil
            }
        }
    }

    private static func codexPaths(_ id: String, home: URL) -> [URL] {
        let manager = FileManager.default
        var matches: [URL] = []
        for root in profileRoots(prefix: ".codex", home: home) {
            let sessions = root.appendingPathComponent("sessions", isDirectory: true)
            guard let files = manager.enumerator(
                at: sessions,
                includingPropertiesForKeys: [.contentModificationDateKey, .isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { continue }
            for case let file as URL in files
            where file.lastPathComponent.hasPrefix("rollout-")
                && file.lastPathComponent.hasSuffix("-\(id).jsonl") {
                matches.append(file)
            }
        }
        return matches
    }

    private static func newest(_ paths: [URL]) -> URL? {
        paths.max { lhs, rhs in
            let left = (try? lhs.resourceValues(forKeys: [.contentModificationDateKey])
                .contentModificationDate) ?? .distantPast
            let right = (try? rhs.resourceValues(forKeys: [.contentModificationDateKey])
                .contentModificationDate) ?? .distantPast
            return left == right ? lhs.path < rhs.path : left < right
        }
    }
}
