import SwarmCore

/// Discovers swarm sessions for the project sidebar while this window is active.
extension AppModel {
    func swarmSession(_ id: SwarmSessionID) -> SwarmProjectSession? {
        swarmSessionsByRepo.values.lazy.flatMap { $0 }.first { $0.id == id }
    }

    /// Owned by the sidebar's active-window task, so leaving the window cancels both the sleep and
    /// any CLI call in progress. Failures use the same bounded backoff as workspace swarm panes.
    func followSwarmSessions() async {
        var failures = 0
        while !Task.isCancelled {
            if await refreshSwarmSessions() {
                failures = 0
            } else {
                failures += 1
            }
            do {
                try await Task.sleep(for: .seconds(
                    SwarmPollSchedule.delay(afterFailures: failures)
                ))
            } catch {
                return
            }
        }
    }

    private func refreshSwarmSessions() async -> Bool {
        guard let store else { return false }
        do {
            let sessions = try await swarmBus.sessions()
            var saved: Set<SwarmSessionID> = []
            for workspace in workspaces {
                if let session = await SwarmWorkspaceSession.load(
                    workspaceID: workspace.id, from: store
                ) {
                    saved.insert(session)
                }
            }
            let discovered = await swarmSessionDiscovery.discover(
                sessions: sessions, repos: repos, excluding: saved
            )
            if discovered != swarmSessionsByRepo { swarmSessionsByRepo = discovered }
            return true
        } catch is CancellationError {
            return false
        } catch {
            return false
        }
    }
}
