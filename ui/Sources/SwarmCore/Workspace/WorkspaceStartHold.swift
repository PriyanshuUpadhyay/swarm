import Foundation

/// The workspaces that have just been created and have not yet started doing anything.
///
/// **Why this exists.** A new workspace went Working, Idle, Working in the pane grouped by status.
/// The pending row stands under Working while the worktree is cut, and `reload` takes it away the
/// moment the stored row arrives. That row has no setup running and no agent turn open yet: the
/// attachments are copied, the opening prompt is queued, the settings are read twice, a port is
/// found and a lease taken before `runStarted` is written, and with no setup script at all nothing
/// is running until the queue drains into the agent. For all of that time the row resolved to
/// `.clean` and sat under Idle. A setup script that succeeds leaves the same gap again between its
/// last line and the agent's first turn.
///
/// So the hold is taken when the worktree exists, in the same update the pending row goes, and
/// given up when an agent is seen running or when the start has finished its work without one: a
/// terminal workspace, a chat with no opening prompt, an archive cancelling the setup. It is held
/// in memory only. A launch that dies mid start leaves nothing behind that would keep a row in
/// Working for good, which is what reading `setupState == .pending` as "about to start" would do.
public struct WorkspaceStartHold: Equatable, Sendable {
    public private(set) var ids: Set<WorkspaceID> = []

    public init() {}

    public func contains(_ id: WorkspaceID) -> Bool {
        ids.contains(id)
    }

    public mutating func begin(_ id: WorkspaceID) {
        ids.insert(id)
    }

    public mutating func release(_ id: WorkspaceID) {
        ids.remove(id)
    }

    /// Lets go of every workspace whose agent is now running, because from here the running set
    /// says what the hold was saying.
    public mutating func settle(running: Set<WorkspaceID>) {
        guard !ids.isDisjoint(with: running) else { return }
        ids.subtract(running)
    }
}
