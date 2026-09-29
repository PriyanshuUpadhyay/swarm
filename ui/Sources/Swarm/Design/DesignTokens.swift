import SwiftUI
import SwarmCore

/// The shared spacing, radius, type, color, and motion values. Views use these, not raw numbers.
enum DesignTokens {
    enum Spacing {
        static let xxs: CGFloat = 2
        static let xs: CGFloat = 4
        static let s: CGFloat = 8
        static let m: CGFloat = 12
        static let l: CGFloat = 16
        static let xl: CGFloat = 24
    }

    enum Radius {
        /// Controls and rows.
        static let control: CGFloat = 6
        /// Cards and panes.
        static let card: CGFloat = 10
        /// Composer and palette.
        static let panel: CGFloat = 14
    }

    static let mono = Font.system(size: 12, design: .monospaced)

    /// For pointer-driven and state-driven changes. Keyboard-driven changes get no animation.
    static let spring = Animation.spring(response: 0.3, dampingFraction: 1)

    static func color(_ status: AgentStatus) -> Color {
        switch status {
        case .working: .accentColor
        case .waiting: .orange
        case .done: .green
        case .failed: .red
        case .ended: .secondary
        }
    }
}
