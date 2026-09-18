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
            if await refreshSwarmSessionsOnce() {
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

    func refreshSwarmSessionsOnce() async -> Bool {
        guard let store else { return false }
        do {
            let sessions = try await swarmBus.sessions()
            var saved: Set<SwarmSessionID> = []
            var localChats: [SwarmSessionID: SessionID] = [:]
            for workspace in workspaces {
                if let session = await SwarmWorkspaceSession.load(
                    workspaceID: workspace.id, from: store
                ) {
                    saved.insert(session)
                }
                for chat in (try? await store.sessions(workspaceID: workspace.id)) ?? [] {
                    if let swarm = await SwarmChatSession.load(sessionID: chat.id, from: store) {
                        localChats[swarm] = chat.id
                    }
                }
            }
            var running = Set(localChats.compactMap { swarm, chat in
                TerminalSessionStore.shared.interactiveState(for: chat) == .stopped ? nil : swarm
            })
            let bus = swarmBus
            await withTaskGroup(of: SwarmSessionID?.self) { group in
                for session in sessions where localChats[session.id] == nil {
                    group.addTask {
                        let agents = try? await bus.agents(in: session)
                        return agents?.contains {
                            $0.id == SwarmAgentID("orchestrator")
                                && $0.pane != nil && $0.alive != false
                        } == true ? session.id : nil
                    }
                }
                for await id in group {
                    if let id { running.insert(id) }
                }
            }
            let discovered = await swarmSessionDiscovery.discover(
                sessions: sessions, repos: repos, workspaces: workspaces,
                localChats: localChats, running: running, excluding: saved
            )
            if discovered != swarmSessionsByRepo { swarmSessionsByRepo = discovered }
            return true
        } catch is CancellationError {
            return false
        } catch {
            return false
        }
    }

    func archiveSwarmChat(_ chat: SwarmProjectSession) async {
        let ids = SwarmSessionListing.archiveIDs(for: chat)
        do {
            try await swarmBus.archive(ids)
        } catch {
            notice = SwarmNotice(message: "Could not archive the chat: \(error.readableMessage)")
            return
        }

        if let workspaceID = chat.workspaceID,
           let localSessionID = chat.localSessionID,
           let workspace = workspaces.first(where: { $0.id == workspaceID }),
           let store {
            do {
                if let session = try await store.session(id: localSessionID) {
                    await model(for: workspace).closeSession(session)
                }
            } catch {
                notice = SwarmNotice(
                    message: "The swarm chat was archived, but its app chat could not close: "
                        + error.readableMessage
                )
            }
        }

        removeSwarmSessions(ids)
        guard selection.swarmSessionID == chat.id else { return }
        if let workspaceID = chat.workspaceID {
            let next = swarmSessionsByRepo.values.lazy.flatMap { $0 }
                .filter { $0.workspaceID == workspaceID }
                .sorted { $0.lastActivity > $1.lastActivity }
                .first
            selection = next.map { .swarmSession($0.id) } ?? .workspace(workspaceID)
        } else {
            selection = .home
        }
    }

    /// Archives the bus rows rooted in one workspace after its worktree archive succeeds.
    func archiveSwarmSessions(inWorkspaceAt path: String) async -> String? {
        do {
            let sessions = try await swarmBus.sessions()
            let ids = SwarmSessionListing.archiveIDs(
                forWorkspaceAt: path, sessions: sessions
            )
            try await swarmBus.archive(ids)
            removeSwarmSessions(ids)
            return nil
        } catch {
            return error.readableMessage
        }
    }

    private func removeSwarmSessions(_ ids: [SwarmSessionID]) {
        let archived = Set(ids)
        for repoID in Array(swarmSessionsByRepo.keys) {
            swarmSessionsByRepo[repoID]?.removeAll { chat in
                chat.sessions.contains { archived.contains($0.id) }
            }
        }
    }
}
