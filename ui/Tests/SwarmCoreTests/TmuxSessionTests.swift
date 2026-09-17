import Foundation
import Testing
@testable import SwarmCore

@Suite("tmux session naming")
struct TmuxSessionNamingTests {
    @Test("A pane always produces the same session name")
    func stable() {
        let workspace = WorkspaceID("9d4b0f1e-1111-2222-3333-444455556666")
        let pane = "6f1c2b8a-0f4c-4a6b-9a1e-2c3d4e5f6071"
        let name = TmuxSessions.sessionName(workspaceID: workspace, paneID: pane)
        #expect(name == TmuxSessions.sessionName(workspaceID: workspace, paneID: pane))
        #expect(name == "swarmui_\(workspace)_\(pane)")
    }

    @Test("Different panes never share a session")
    func unique() {
        let workspace = WorkspaceID.new()
        let names = Set((0..<200).map { _ in
            TmuxSessions.sessionName(workspaceID: workspace, paneID: newID())
        })
        #expect(names.count == 200)
    }

    @Test("A session name round trips back to its workspace and its pane")
    func roundTrip() {
        let workspace = WorkspaceID.new()
        let pane = newID()
        let name = TmuxSessions.sessionName(workspaceID: workspace, paneID: pane)
        #expect(TmuxSessions.paneID(ofSessionName: name) == pane)
        #expect(TmuxSessions.workspaceID(ofSessionName: name) == workspace.rawValue)
    }

    @Test("Only Swarm's own shape is recognised, so a user's sessions are never touched")
    func foreignSessions() {
        #expect(TmuxSessions.isSwarmSession(TmuxSessions.sessionName(workspaceID: WorkspaceID.new(), paneID: newID())))
        #expect(!TmuxSessions.isSwarmSession("work"))
        #expect(!TmuxSessions.isSwarmSession("swarmuiish"))
        #expect(!TmuxSessions.isSwarmSession("my_swarmui_thing"))
        #expect(!TmuxSessions.isSwarmSession("swarmui_"))
        #expect(!TmuxSessions.isSwarmSession("swarmui__pane"))
        #expect(!TmuxSessions.isSwarmSession("swarmui_ws_pane_extra"))
        #expect(TmuxSessions.paneID(ofSessionName: "0") == nil)
    }

    // tmux treats the first two as target separators, and the underscore is what splits a name
    // back into its fields, so none of them may survive into an id.
    @Test("Characters that would break a name are folded away", arguments: [".", ":", " ", "$", "_"])
    func unsafeCharacters(character: String) {
        let name = TmuxSessions.sessionName(workspaceID: WorkspaceID("ws"), paneID: "pane\(character)one")
        #expect(name == "swarmui_ws_pane-one")
        #expect(TmuxSessions.paneID(ofSessionName: name) == "pane-one")
    }

    @Test("The socket is per database, so a throwaway instance cannot see the real one")
    func socketPerDatabase() {
        let real = TmuxSessions.socketName(databasePath: "/Users/x/Library/Application Support/Swarm/swarm.sqlite")
        let copy = TmuxSessions.socketName(databasePath: "/tmp/scratch/swarm.sqlite")
        #expect(real != copy)
        #expect(real == TmuxSessions.socketName(databasePath: "/Users/x/Library/Application Support/Swarm/swarm.sqlite"))
        #expect(real.hasPrefix("swarmui-"))
    }

    @Test("The fingerprint does not move between runs")
    func fingerprintIsStable() {
        // A literal, not a recomputation: the point is that a socket named by an older launch is
        // still found by this one. `Hashable` would pass a self-comparison and fail this.
        #expect(TmuxSessions.fingerprint("/tmp/swarm.sqlite") == "c7d8035a")
        #expect(TmuxSessions.fingerprint("") == "811c9dc5")
    }
}

@Suite("tmux restore decision")
struct TmuxRestoreDecisionTests {
    private let workspace = WorkspaceID("0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9")
    private let pane = "1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed"
    private var name: String { TmuxSessions.sessionName(workspaceID: workspace, paneID: pane) }

    @Test("The setting off keeps the historical in-process shell")
    func settingOff() {
        let decision = TmuxSessions.decide(
            workspaceID: workspace, paneID: pane,
            persistenceEnabled: false, tmuxAvailable: true, existingSessions: []
        )
        #expect(decision == .inProcess)
    }

    @Test("No tmux on the machine falls back rather than failing")
    func noTmux() {
        let decision = TmuxSessions.decide(
            workspaceID: workspace,
            paneID: pane,
            persistenceEnabled: true,
            tmuxAvailable: false,
            existingSessions: [name]
        )
        #expect(decision == .inProcess)
    }

    @Test("An agent chat uses tmux even when terminal persistence is off")
    func agentChatRequiresTmux() {
        let decision = TmuxSessions.decide(
            workspaceID: workspace,
            paneID: pane,
            persistenceEnabled: false,
            requiresTmux: true,
            tmuxAvailable: true,
            existingSessions: []
        )
        #expect(decision == .createFresh(session: name))
    }

    @Test("A session that survived the last quit is reattached")
    func reattach() {
        let decision = TmuxSessions.decide(
            workspaceID: workspace,
            paneID: pane,
            persistenceEnabled: true,
            tmuxAvailable: true,
            existingSessions: [name, "swarmui_other_pane"]
        )
        #expect(decision == .attach(session: name))
    }

    @Test("A pane whose session is gone starts fresh instead of erroring")
    func startFresh() {
        let decision = TmuxSessions.decide(
            workspaceID: workspace,
            paneID: pane,
            persistenceEnabled: true,
            tmuxAvailable: true,
            existingSessions: ["swarmui_someone_else"]
        )
        #expect(decision == .createFresh(session: name))
    }

    @Test("Both tmux outcomes exec the same command, so a stale snapshot cannot break a pane")
    func sameArguments() {
        let command = TmuxCommand(executable: "/opt/homebrew/bin/tmux", socketName: "swarmui-1", configPath: "/c")
        let attach = TmuxSessions.decide(
            workspaceID: workspace, paneID: pane, persistenceEnabled: true, tmuxAvailable: true,
            existingSessions: [name]
        )
        let fresh = TmuxSessions.decide(
            workspaceID: workspace, paneID: pane, persistenceEnabled: true, tmuxAvailable: true,
            existingSessions: []
        )
        let arguments = { (decision: TerminalStartDecision) in
            command.attachOrCreate(session: decision.session ?? "", directory: "/w", environment: [:])
        }
        #expect(arguments(attach) == arguments(fresh))
    }
}

@Suite("tmux orphan rule")
struct TmuxOrphanTests {
    private let workspace = WorkspaceID.new()

    private func session(_ pane: String) -> String {
        TmuxSessions.sessionName(workspaceID: workspace, paneID: pane)
    }

    @Test("A session whose pane is still reachable is kept")
    func keepsLive() {
        let pane = newID()
        #expect(TmuxSessions.orphans(sessions: [session(pane)], livePaneIDs: [pane], sparing: []).isEmpty)
    }

    @Test("A session whose pane is gone is swept")
    func sweepsDeadPane() {
        let live = newID()
        let dead = newID()
        let orphans = TmuxSessions.orphans(
            sessions: [session(live), session(dead)], livePaneIDs: [live], sparing: []
        )
        #expect(orphans == [session(dead)])
    }

    @Test("A session belonging to a workspace that is gone is swept with it")
    func sweepsArchivedWorkspace() {
        // The caller builds the live set from the workspaces still in the database, so an archived
        // workspace contributes no panes and every session it owned reads as unreachable.
        let archived = [newID(), newID()]
        let surviving = newID()
        let orphans = TmuxSessions.orphans(
            sessions: (archived + [surviving]).map(session), livePaneIDs: [surviving], sparing: []
        )
        #expect(Set(orphans) == Set(archived.map(session)))
    }

    @Test("A session the user created themselves is never swept")
    func leavesForeignSessions() {
        let orphans = TmuxSessions.orphans(
            sessions: ["work", "dotfiles", "0", "swarmui", "swarmui_only-two-fields"], livePaneIDs: [], sparing: []
        )
        #expect(orphans.isEmpty)
    }

    @Test("Nothing live means nothing survives, which is what an empty database should do")
    func sweepsEverythingWhenNothingIsLive() {
        let sessions = [newID(), newID()].map(session)
        #expect(TmuxSessions.orphans(sessions: sessions, livePaneIDs: [], sparing: []) == sessions)
    }

    @Test("Turning the setting off makes every session unreachable, so the sweep takes them all")
    func settingOffSweepsEverything() {
        let live = newID()
        let panes = TmuxSessions.reachablePanes([live], persistenceEnabled: false)
        #expect(panes.isEmpty)
        #expect(TmuxSessions.orphans(sessions: [session(live)], livePaneIDs: panes, sparing: []) == [session(live)])
    }

    @Test("With the setting on, a live pane stays reachable")
    func settingOnKeepsLivePanes() {
        let live = newID()
        #expect(TmuxSessions.reachablePanes([live], persistenceEnabled: true) == [live])
    }

    /// The whole point of the doubt channel. A workspace whose stored tab list would not decode
    /// contributes no pane ids, so without this it reads exactly like an archived workspace and
    /// every shell it holds is killed. A renamed coding key on `CenterTab` is one edit away from
    /// that, and a killed shell cannot be got back.
    @Test("A workspace whose panes could not be enumerated keeps every session it owns")
    func sparesDoubtfulWorkspace() {
        let unknown = [newID(), newID()]
        #expect(TmuxSessions.orphans(
            sessions: unknown.map(session), livePaneIDs: [], sparing: [workspace]
        ).isEmpty)
    }

    /// Sparing is per workspace, not a blanket amnesty: the workspaces that did read cleanly are
    /// still swept, so one unreadable record cannot stop the sweep collecting anything at all.
    @Test("Sparing one workspace does not spare another")
    func sparingIsPerWorkspace() {
        let other = WorkspaceID.new()
        let dead = TmuxSessions.sessionName(workspaceID: other, paneID: newID())
        let orphans = TmuxSessions.orphans(
            sessions: [session(newID()), dead], livePaneIDs: [], sparing: [workspace]
        )
        #expect(orphans == [dead])
    }

    /// Doubt outranks the setting. Off means the shells of every workspace whose panes are known
    /// go; it cannot mean "kill what nothing could read", because that is the answer whether the
    /// shells are wanted or not.
    @Test("Turning the setting off still does not sweep a workspace nobody could enumerate")
    func settingOffStillSparesDoubt() {
        let live = newID()
        let panes = TmuxSessions.reachablePanes([live], persistenceEnabled: false)
        #expect(TmuxSessions.orphans(
            sessions: [session(live)], livePaneIDs: panes, sparing: [workspace]
        ).isEmpty)
    }

    @Test("list-sessions output is read line by line")
    func parsesList() {
        #expect(TmuxSessions.parseSessionList("swarmui_a_b\nswarmui_c_d\n") == ["swarmui_a_b", "swarmui_c_d"])
        #expect(TmuxSessions.parseSessionList("") == [])
        // What tmux prints when no server is running arrives on stderr, so stdout is empty.
        #expect(TmuxSessions.parseSessionList("\n\n") == [])
    }
}

@Suite("tmux archive teardown")
struct TmuxArchiveTeardownTests {
    @Test("Archiving names every session of that workspace and nothing else")
    func archiveTargetsTheWorkspace() {
        let archived = WorkspaceID.new()
        let other = WorkspaceID.new()
        let doomed = (0..<3).map { _ in TmuxSessions.sessionName(workspaceID: archived, paneID: newID()) }
        let spared = [
            TmuxSessions.sessionName(workspaceID: other, paneID: newID()),
            "someone-elses-session",
        ]

        let killed = TmuxSessions.sessions(ofWorkspace: archived, in: doomed + spared)
        #expect(Set(killed) == Set(doomed))
    }

    @Test("An archived workspace leaves no tmux session behind")
    func nothingSurvivesAnArchive() {
        let archived = WorkspaceID.new()
        var live = (0..<4).map { _ in TmuxSessions.sessionName(workspaceID: archived, paneID: newID()) }

        // What `TerminalSessionStore.discard` does: match by workspace, kill, then look again.
        for session in TmuxSessions.sessions(ofWorkspace: archived, in: live) {
            live.removeAll { $0 == session }
        }
        #expect(TmuxSessions.sessions(ofWorkspace: archived, in: live).isEmpty)
        #expect(live.isEmpty)
    }

    @Test("A workspace whose panes were never drawn is still torn down")
    func doesNotDependOnLoadedTabs() {
        // The names are the only bookkeeping this path trusts. A tab list that was never loaded,
        // or a split layout that was lost, cannot leave a shell alive in a deleted worktree.
        let archived = WorkspaceID.new()
        let session = TmuxSessions.sessionName(workspaceID: archived, paneID: newID())
        #expect(TmuxSessions.sessions(ofWorkspace: archived, in: [session]) == [session])
    }

    @Test("A workspace with no terminals leaves nothing to kill")
    func noSessions() {
        #expect(TmuxSessions.sessions(ofWorkspace: WorkspaceID.new(), in: []).isEmpty)
    }
}

@Suite("tmux command lines")
struct TmuxCommandTests {
    private let command = TmuxCommand(
        executable: "/opt/homebrew/bin/tmux", socketName: "swarm-deadbeef", configPath: "/cfg/tmux.conf"
    )

    @Test("Every command names the private socket and the generated config")
    func globals() {
        #expect(command.globalArguments == ["-L", "swarm-deadbeef", "-f", "/cfg/tmux.conf", "-u"])
        #expect(command.listSessions.starts(with: command.globalArguments))
        #expect(command.killSession("swarm-x").starts(with: command.globalArguments))
    }

    @Test("A pane attaches or creates in one exec, with its workspace environment")
    func attachOrCreate() {
        let arguments = command.attachOrCreate(
            session: "swarm-x",
            directory: "/tmp/work",
            environment: ["SWARM_UI_PORT": "3000", "CONDUCTOR_PORT": "3000"]
        )
        #expect(arguments == [
            "-L", "swarm-deadbeef", "-f", "/cfg/tmux.conf", "-u",
            "set-environment", "-gr", "NO_COLOR", ";",
            "new-session", "-A", "-D", "-s", "swarm-x", "-c", "/tmp/work",
            "-e", "CONDUCTOR_PORT=3000",
            "-e", "SWARM_UI_PORT=3000",
        ])
    }

    @Test("A detached chair starts registration and the CLI in its named session")
    func detachedChair() {
        let plan = SwarmChairLaunchPlan(
            workspaceID: WorkspaceID("workspace"), sessionID: SessionID("chat"),
            paneID: TerminalTabID("pane"), tmuxSession: "swarmui-workspace-pane",
            directory: "/tmp/work", executable: "/usr/bin/env",
            arguments: ["codex", "--", "Fix the launch"],
            environment: ["SWARM_SESSION_ID": "42"]
        )
        let arguments = command.launchDetached(plan)

        #expect(arguments.contains("swarmui-workspace-pane"))
        #expect(arguments.contains("SWARM_SESSION_ID=42"))
        #expect(arguments.suffix(3) == [
            "/bin/sh", "-c",
            "swarm agent add orchestrator orchestrator && exec '/usr/bin/env' 'codex' '--' 'Fix the launch'",
        ])
    }

    @Test("Composer input targets the agent pane and uses a named paste buffer")
    func agentInput() {
        let textArguments = command.pasteBuffer("composer", intoAgentPaneOf: "swarm-x")
        let returnArguments = command.send(.enter, toAgentPaneOf: "swarm-x")
        #expect(textArguments == [
            "-L", "swarm-deadbeef", "-f", "/cfg/tmux.conf", "-u",
            "load-buffer", "-b", "composer", "-", ";",
            "paste-buffer", "-d", "-p", "-b", "composer", "-t", "=swarm-x:0.0",
        ])
        #expect(returnArguments.suffix(4) == ["send-keys", "-t", "=swarm-x:0.0", "Enter"])
    }

    @Test("Kills name the session exactly, so a prefix match can never take a second one")
    func exactKill() {
        #expect(command.killSession("swarm-x").suffix(2) == ["-t", "=swarm-x"])
    }

    @Test("The generated configuration turns off everything that would be noticed")
    func configuration() {
        let text = TmuxSessions.configuration(defaultShell: "/bin/zsh")
        #expect(text.contains("set -g default-shell \"/bin/zsh\""))
        #expect(text.contains("set -g escape-time 0"))
        #expect(text.contains("set -g status off"))
        #expect(text.contains("set -g prefix None"))
        #expect(text.contains("set -g prefix2 None"))
        #expect(text.contains("unbind-key -a -T prefix"))
        // The wheel is the one thing that must reach tmux: a client puts SwiftTerm into its
        // alternate screen, so without this a scroll would do nothing at all.
        #expect(text.contains("set -g mouse on"))
        #expect(text.contains("set -g set-clipboard on"))
        #expect(!text.contains("unset-environment TMUX"))
        #expect(!text.contains("unset-environment TMUX_PANE"))
    }
}

@Suite("tmux pane pids")
struct TmuxPanePIDTests {
    private let command = TmuxCommand(
        executable: "/opt/homebrew/bin/tmux",
        socketName: "swarm-deadbeef",
        configPath: "/cfg/tmux.conf"
    )

    @Test("The whole server is asked at once, pid first")
    func arguments() {
        #expect(command.listPanes == [
            "-L", "swarm-deadbeef", "-f", "/cfg/tmux.conf", "-u",
            "list-panes", "-a", "-F", "#{pane_pid} #{session_name}",
        ])
    }

    @Test("A pane line reads back as its session and the pid of the shell in it")
    func parsing() {
        let pids = TmuxSessions.parsePanePIDs("""
            40123 swarm_ws_one
            40200 swarm_ws_two
            """)
        #expect(pids == ["swarm_ws_one": 40123, "swarm_ws_two": 40200])
    }

    // The pid leads so it cannot be lost: a name is only ours by convention, and a name with a
    // space in it read the other way round would take the pid with it.
    @Test("A session name with a space in it keeps its pid")
    func spacedName() {
        #expect(TmuxSessions.parsePanePIDs("77 my session") == ["my session": 77])
    }

    @Test("Anything that is not a pid and a name is skipped")
    func rubbish() {
        #expect(TmuxSessions.parsePanePIDs("") == [:])
        #expect(TmuxSessions.parsePanePIDs("no server running\nswarm_a_b\n") == [:])
        #expect(TmuxSessions.parsePanePIDs("40123") == [:])
    }

    @Test("The first pane of a session wins, because ours hold exactly one")
    func firstPane() {
        #expect(TmuxSessions.parsePanePIDs("40123 swarm_a_b\n40124 swarm_a_b") == ["swarm_a_b": 40123])
    }
}
