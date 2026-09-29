import Foundation

/// Performance gate aid (ADR 0022): `SWARM_PANE_STRESS=N` opens a window whose strip holds N panes
/// that stream local output, so the strip can be measured without real agents.
public enum SwarmPaneStress {
    public static var count: Int {
        let value = Int(ProcessInfo.processInfo.environment["SWARM_PANE_STRESS"] ?? "") ?? 0
        return min(max(value, 0), 24)
    }

    /// `SWARM_PANE_STRESS_SCROLL=1` sweeps the strip left and right, for traces without input.
    public static var scrolls: Bool { ["1", "end"].contains(scrollMode) }

    /// `end` parks the strip at its right end instead, for screenshots of a background window,
    /// where App Nap slows the sweep's timer.
    public static var parksAtEnd: Bool { scrollMode == "end" }

    private static var scrollMode: String? { ProcessInfo.processInfo.environment["SWARM_PANE_STRESS_SCROLL"] }

    public static var launch: SwarmAttachLaunch {
        // Repaint in place from the top, as tmux and agent TUIs do; a scrolling flood of raw lines
        // looks like flicker and is not what a real pane shows. The cursor is hidden, as it would jump with each frame.
        let script = #"printf '\033[?25l\033[2J'; while :; do printf '\033[H'; date; ls -la /usr/bin | head -20 | sed 's/$/\x1b[K/'; printf '\033[J'; sleep 0.05; done"#
        return SwarmAttachLaunch(
            command: SwarmAttachCommand(executable: "/bin/sh", arguments: ["-c", script], environment: [:]),
            directory: NSHomeDirectory()
        )
    }
}
