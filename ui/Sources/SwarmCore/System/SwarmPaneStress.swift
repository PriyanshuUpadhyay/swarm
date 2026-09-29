import Foundation

/// Performance gate aid (ADR 0022): `SWARM_PANE_STRESS=N` opens a window whose strip holds N chat
/// columns, so the strip can be measured without real agents.
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

    /// `SWARM_PANE_STRESS_LOG=<claude .jsonl>`: the chat every stress column reads.
    public static var log: String? { ProcessInfo.processInfo.environment["SWARM_PANE_STRESS_LOG"] }
}
