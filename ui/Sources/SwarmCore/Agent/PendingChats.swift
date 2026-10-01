import Foundation

/// Why a chat start failed, as the app shows it.
public struct LaunchFailure: Sendable, Hashable {
    public let message: String
    /// Every runner of the profile was skipped because its CLI is not installed, so the app adds
    /// an install hint under swarm's text.
    public let missingCLI: Bool

    public init(_ error: any Error) {
        self.init(message: (error as? SwarmProfileError)?.message ?? String(describing: error))
    }

    public init(message: String) {
        self.message = message
        missingCLI = Self.isMissingCLI(message)
    }

    /// Matches `swarm launch`'s text when no runner can run (`resolve_role` in `src/main.rs`):
    /// a `swarm: <role>: no runner can run` line, then one line per runner. A runner with no CLI
    /// says `<provider> CLI not found on PATH`.
    static func isMissingCLI(_ message: String) -> Bool {
        let lines = message.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
        guard let head = lines.firstIndex(where: {
            $0.hasPrefix("swarm: ") && $0.hasSuffix(": no runner can run")
        }) else { return false }
        let runners = lines[(head + 1)...].filter { !$0.isEmpty }
        return !runners.isEmpty && runners.allSatisfy { $0.hasSuffix("CLI not found on PATH") }
    }
}

/// A chat the app is starting. It shows as its own tab until the chair is up and the tree has its
/// session row (ADR 0035).
public struct PendingChat: Identifiable, Sendable, Equatable {
    public enum State: Sendable, Equatable {
        case starting
        /// The chair is up; the tab waits for the tree to list the session.
        case launched
        case failed(LaunchFailure)
        /// Close is archiving the session of this failed start; Retry and Close wait for it.
        case closing(LaunchFailure)
    }

    /// The tab that was selected when a start began: a chat, or another start.
    public enum Previous: Sendable, Equatable {
        case session(SwarmSessionID)
        case pending(UUID)
    }

    public let id: UUID
    public let directory: String
    /// The tab that was selected when this start began, for Close to return to.
    public let previous: Previous?
    /// Set when `session new` returns, so Retry launches in it and Close archives it.
    public var session: SwarmSessionID?
    public var state: State

    public var tabID: String { "pending:" + id.uuidString }
}

/// The chats the app is starting, in the order they began.
public struct PendingChats: Sendable, Equatable {
    public private(set) var items: [PendingChat] = []

    public init() {}

    public subscript(id: UUID) -> PendingChat? { items.first { $0.id == id } }

    public mutating func add(directory: String, previous: PendingChat.Previous?) -> UUID {
        let chat = PendingChat(
            id: UUID(), directory: directory, previous: previous, session: nil, state: .starting
        )
        items.append(chat)
        return chat.id
    }

    public mutating func update(_ id: UUID, _ change: (inout PendingChat) -> Void) {
        guard let index = items.firstIndex(where: { $0.id == id }) else { return }
        change(&items[index])
    }

    public mutating func remove(_ id: UUID) -> PendingChat? {
        guard let index = items.firstIndex(where: { $0.id == id }) else { return nil }
        return items.remove(at: index)
    }

    /// Removes and returns each launched chat whose session row the tree now lists.
    public mutating func settle(listed: (SwarmSessionID) -> Bool) -> [PendingChat] {
        let done = items.filter { $0.state == .launched && $0.session.map(listed) == true }
        items.removeAll { chat in done.contains { $0.id == chat.id } }
        return done
    }

    public func inWorkspace(_ directory: String) -> [PendingChat] {
        items.filter { $0.directory == directory }
    }

    /// Sessions that a pending tab stands for. Their rows stay out of the tab strip, so one chat
    /// never shows as two tabs.
    public var sessions: Set<SwarmSessionID> { Set(items.compactMap(\.session)) }
}

/// Runs one piece of async work at a time, in call order. An actor alone does not do this: it lets
/// another call in at each `await`.
actor SerialGate {
    private var tail: Task<Void, Never>?

    func run<Value: Sendable>(_ work: @escaping @Sendable () async throws -> Value) async throws -> Value {
        let previous = tail
        let task = Task {
            await previous?.value
            return try await work()
        }
        tail = Task { _ = try? await task.value }
        return try await task.value
    }
}
