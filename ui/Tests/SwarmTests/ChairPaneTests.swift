import Foundation
@testable import SwarmCore
import Testing

struct ChairPaneTests {
    @Test func soloReportsItsOwnPane() throws {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let adapter = try String(
            contentsOf: repository.appendingPathComponent("adapters/tmux-solo.conf"),
            encoding: .utf8
        )
        #expect(adapter.split(separator: "\n").contains("self = printf '%s' \"$TMUX_PANE\""))
    }

    @Test func chairStartsOnTheSwarmServer() {
        let plainTabs = TmuxCommand(
            executable: "/usr/bin/tmux", socketName: "swarmui-example", configPath: "/tmp/tmux.conf"
        )
        let chair = plainTabs.swarmChair
        let plan = SwarmChairLaunchPlan(
            workspaceID: WorkspaceID("workspace"), sessionID: SessionID("chat"),
            paneID: TerminalTabID("chair-pane"), tmuxSession: "swarmui-workspace-chair-pane",
            directory: "/tmp/work", executable: "/usr/bin/env", arguments: ["codex"],
            environment: ["SWARM_ADAPTER": "tmux-solo"]
        )
        #expect(chair.launchDetached(plan).prefix(2) == ["-L", "swarm"])
    }

    @Test func plainTabsKeepTheAppSocket() {
        let databasePath = "/tmp/swarm-ui.sqlite"
        let socket = TmuxSessions.socketName(databasePath: databasePath)
        #expect(socket == "swarmui-" + TmuxSessions.fingerprint(databasePath))
        #expect(socket != TmuxSessions.swarmSocketName)
    }
}
