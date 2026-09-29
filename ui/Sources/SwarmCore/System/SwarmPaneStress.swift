import Foundation

/// Performance gate aid (ADR 0022): `SWARM_PANE_STRESS=N` opens a window whose strip holds N panes
/// that stream local output, so the strip can be measured without real agents.
public enum SwarmPaneStress {
    public static var count: Int {
        let value = Int(ProcessInfo.processInfo.environment["SWARM_PANE_STRESS"] ?? "") ?? 0
        return min(max(value, 0), 24)
    }

    /// `SWARM_PANE_STRESS_SCROLL=1` sweeps the strip left and right, for traces without input.
    public static var scrolls: Bool {
        ProcessInfo.processInfo.environment["SWARM_PANE_STRESS_SCROLL"] == "1"
    }

    public static var launch: SwarmAttachLaunch {
        let script = "while :; do date; ls -la /usr/bin | head -40; sleep 0.05; done"
        return SwarmAttachLaunch(
            command: SwarmAttachCommand(executable: "/bin/sh", arguments: ["-c", script], environment: [:]),
            directory: NSHomeDirectory()
        )
    }
}
