import SwiftUI
import SwarmCore

/// One agent status as a symbol and a color, so color is never the only signal.
struct StatusGlyph: View {
    let status: AgentStatus
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Image(systemName: Self.symbol(status))
            .foregroundStyle(DesignTokens.color(status))
            .symbolEffect(
                .rotate, options: .repeat(.continuous).speed(0.3),
                isActive: status == .working && !reduceMotion
            )
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
