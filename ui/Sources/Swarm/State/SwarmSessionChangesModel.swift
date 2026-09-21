import Foundation
import Observation
import SwarmCore

/// The uncommitted work in a swarm session's own folder.
///
/// **A session is not a workspace, which is why this is not the inspector.** Every changed-file
/// view in this app takes a `WorkspaceModel`: the list, the diff, the revert and the review pane
/// all hang off one. A session started from the command line has no workspace row, no base branch
/// and nothing to merge into, so handing those views an invented `Workspace` would be inventing
/// a base branch as well. What a session does have is a working directory, and `git` can be asked
/// what is uncommitted in one without being told anything else. That is the whole of what this
/// reads.
///
/// **Not every session has a repository either.** On this Mac most sessions run from the home
/// directory, which is not a checkout, so "no repository" is a first class answer rather than an
/// error: see `isRepository`.
@MainActor
@Observable
final class SwarmSessionChangesModel {
    /// The folder the session is running in, exactly as the bus recorded it.
    let cwd: String

    private(set) var files: [ChangedFile] = []
    /// Nil until the first read finishes, so the pane can say it is reading rather than say there
    /// is nothing.
    private(set) var hasRead = false
    /// False once git has said this folder is not in a checkout. The pane then says so and stops
    /// asking, because the answer cannot change while the session is open.
    private(set) var isRepository = true
    private(set) var failure: String?

    private var reading: Task<Void, Never>?

    init(cwd: String) {
        self.cwd = cwd
    }

    /// What the header counts, which is files rather than lines: a session's folder can hold a
    /// build directory nobody wants a line count of.
    var summary: String {
        Counted.of(files.count, "file")
    }

    /// Reads once now, then follows on a slow clock.
    ///
    /// Five seconds, and a clock rather than FSEvents, which is the opposite of the choice
    /// `SwarmSessionReaderModel` makes and is deliberate. The bus writes one small file per
    /// message and a watcher on it is cheap; a working tree is written to by a compiler, so a
    /// watcher on one fires hundreds of times during a build and each wake costs four `git`
    /// processes. A reader looking at a diff does not need it inside a second.
    func follow() async {
        while !Task.isCancelled {
            await refresh()
            guard isRepository else { return }
            try? await Task.sleep(for: .seconds(5))
        }
    }

    func refresh() async {
        reading?.cancel()
        let task = Task { await read() }
        reading = task
        await task.value
    }

    private func read() async {
        guard await Git.isRepository(cwd) else {
            isRepository = false
            hasRead = true
            files = []
            failure = nil
            return
        }
        do {
            let found = try await Git.uncommittedFiles(worktree: cwd)
            guard !Task.isCancelled else { return }
            files = found
            failure = nil
        } catch {
            guard !Task.isCancelled else { return }
            failure = error.readableMessage
        }
        isRepository = true
        hasRead = true
    }
}
