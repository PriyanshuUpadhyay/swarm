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

    enum Size {
        /// A one-line sidebar row.
        static let row: CGFloat = 28
        /// A pane header.
        static let paneHeader: CGFloat = 24
        static let tabMinWidth: CGFloat = 120
        static let tabMaxWidth: CGFloat = 220
        /// A fixed slot for a status glyph, so titles line up with or without one.
        static let glyphSlot: CGFloat = 16
        /// A small square control, such as an icon button.
        static let iconButton: CGFloat = 26
        /// The composer's slash and mention menu.
        static let menuWidth: CGFloat = 480
        static let sheet: CGFloat = 480
        static let sheetHeight: CGFloat = 340
        static let narrowSheet: CGFloat = 420
        /// The profile editor: one runner row holds provider, model, effort, and a remove button.
        static let profileSheet: CGFloat = 680
        /// The "1  Primary" label before a runner row's pickers; the flags row starts under the pickers.
        static let runnerLabel: CGFloat = 104
        /// A model or account list inside a sheet.
        static let pickerList: CGFloat = 190
        /// Tool output and long text before Show full output.
        static let outputPreview: CGFloat = 300
        static let collapsedOutput: CGFloat = 44
        static let segmentedPicker: CGFloat = 150
        static let quoteBar: CGFloat = 3
        /// A resize grip's hit area: the sidebar edge and the pane strip dividers.
        static let dragHandle: CGFloat = 7
        /// The command palette and its result list.
        static let palette: CGFloat = 640
        static let paletteResults: CGFloat = 420
        /// How far below the window top the palette opens.
        static let paletteTop: CGFloat = 72
        static let focusRing: CGFloat = 2
        static let hairline: CGFloat = 1
    }

    /// Body text is 13 pt; this spacing gives it about 1.45 line height.
    static let body = Font.system(size: 13)
    static let bodyLineSpacing: CGFloat = 5.5
    static let mono = Font.system(size: 12, design: .monospaced)

    /// Quiet fills: a selected row, a user message, a match highlight.
    static let selectionFill = Color.primary.opacity(0.08)
    static let userMessageFill = Color.primary.opacity(0.05)
    static let matchFill = Color.yellow.opacity(0.14)
    static let currentMatchFill = Color.accentColor.opacity(0.28)
    static let endedPaneOpacity = 0.6
    static let selectionAccentFill = Color.accentColor.opacity(0.15)
    static let rawBadgeFill = Color.orange.opacity(0.2)
    static let quoteBar = Color.secondary.opacity(0.5)
    static let codeHeaderFill = Color.primary.opacity(0.05)
    static let codeBlockFill = Color.primary.opacity(0.03)
    /// A question an agent waits on, in the waiting status colour.
    static let promptFill = color(.waiting).opacity(0.08)
    static let promptBorder = color(.waiting).opacity(0.4)
    /// A chat's question strip before it scrolls.
    static let promptListMaxHeight: CGFloat = 280

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

extension View {
    /// Chrome (sidebar, composer, pills) sits on Liquid Glass; under Reduce Transparency it is a
    /// solid window surface. Content stays solid.
    func chromeSurface(in shape: some Shape = Rectangle()) -> some View {
        modifier(ChromeSurface(shape: shape))
    }
}

private struct ChromeSurface<S: Shape>: ViewModifier {
    let shape: S
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    func body(content: Content) -> some View {
        if reduceTransparency {
            content.background(Color(nsColor: .windowBackgroundColor), in: shape)
        } else {
            content.glassEffect(.regular, in: shape)
        }
    }
}
