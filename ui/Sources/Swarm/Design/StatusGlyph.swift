import SwiftUI
import SwarmCore

/// One agent status as a symbol (a spinner while working) and a color, so color is never the only
/// signal.
struct StatusGlyph: View {
    let status: AgentStatus
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var spins: Bool { status == .working && !reduceMotion }

    var body: some View {
        Image(systemName: Self.symbol(status))
            .foregroundStyle(DesignTokens.color(status))
            // An AppKit spinner, because a repeating symbol effect re-renders the whole window on
            // every frame. The hidden symbol keeps the size the caller's font gives.
            .opacity(spins ? 0 : 1)
            .overlay {
                if spins { ProgressView().controlSize(.mini) }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Self.title(status))
            .help(Self.title(status))
    }

    static func symbol(_ status: AgentStatus) -> String {
        switch status {
        // A spinner shape reads as in progress even when still; a dotted ring read as empty.
        case .working: "progress.indicator"
        case .waiting: "exclamationmark.circle.fill"
        case .done: "checkmark.circle"
        case .failed: "xmark.octagon.fill"
        case .ended: "circle"
        }
    }

    static func title(_ status: AgentStatus) -> String {
        switch status {
        case .working: "Working"
        case .waiting: "Waiting for you"
        case .done: "Done"
        case .failed: "Failed"
        case .ended: "Ended"
        }
    }
}
