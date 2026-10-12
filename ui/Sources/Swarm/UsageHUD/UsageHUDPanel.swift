import AppKit
import SwiftUI
import SwarmCore

private typealias HUD = DesignTokens.UsageHUD

/// What the HUD asks its window to do.
struct UsageHUDActions {
    var refresh: () -> Void
    var close: () -> Void
    var openAccounts: () -> Void
    /// A drag of a card edge ended, so the hover area must follow the new size.
    var resizeEnded: () -> Void
}

extension UsageSeverity {
    /// nil for calm: a calm value keeps the color of its place.
    var hudColor: Color? {
        switch self {
        case .danger: HUD.danger
        case .warn: HUD.warn
        case .calm: nil
        }
    }
}

/// The notch outline: shoulders that curve out at the top, and round bottom corners.
private struct NotchBackdropShape: Shape {
    var topShoulder: CGFloat
    var bottomRadius: CGFloat

    var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get { AnimatablePair(topShoulder, bottomRadius) }
        set {
            topShoulder = newValue.first
            bottomRadius = newValue.second
        }
    }

    func path(in rect: CGRect) -> Path {
        let shoulder = min(topShoulder, rect.height / 3)
        let radius = min(bottomRadius, rect.height / 2)
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + shoulder, y: rect.minY + shoulder),
            control: CGPoint(x: rect.minX + shoulder, y: rect.minY)
        )
        path.addLine(to: CGPoint(x: rect.minX + shoulder, y: rect.maxY - radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + shoulder + radius, y: rect.maxY),
            control: CGPoint(x: rect.minX + shoulder, y: rect.maxY)
        )
        path.addLine(to: CGPoint(x: rect.maxX - shoulder - radius, y: rect.maxY))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX - shoulder, y: rect.maxY - radius),
            control: CGPoint(x: rect.maxX - shoulder, y: rect.maxY)
        )
        path.addLine(to: CGPoint(x: rect.maxX - shoulder, y: rect.minY + shoulder))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY),
            control: CGPoint(x: rect.maxX - shoulder, y: rect.minY)
        )
        path.closeSubpath()
        return path
    }
}

/// The usage HUD in its window: the notch shape when collapsed, the card under it when open.
struct UsageHUDPanel: View {
    let model: UsageHUDModel
    let actions: UsageHUDActions
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    private var isCollapsed: Bool { model.isCollapsed }

    private var currentSize: CGSize {
        isCollapsed
            ? CGSize(width: model.notchTriggerWidth, height: model.notchTopInset)
            : CGSize(width: model.panelWidth, height: model.notchTopInset + model.expandedContentHeight)
    }

    /// Opening can go a little past its end. Closing must look like the panel only left.
    private var geometryAnimation: Animation? {
        guard !reduceMotion else { return nil }
        return isCollapsed
            ? .spring(response: 0.3, dampingFraction: 1)
            : .spring(response: 0.36, dampingFraction: 0.86)
    }

    private var backdrop: NotchBackdropShape {
        NotchBackdropShape(
            topShoulder: isCollapsed ? HUD.collapsedShoulder : HUD.openShoulder,
            bottomRadius: isCollapsed ? HUD.collapsedCorner : HUD.openCorner
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            // A notch covers this area. With no notch it is the pill under the menu bar.
            Group {
                if model.hasNotch { Color.clear } else { pill }
            }
            .frame(height: model.notchTopInset)

            if !isCollapsed {
                UsageHUDCard(model: model, actions: actions, reduceMotion: reduceMotion)
                    .transition(reduceMotion
                                ? .opacity
                                : .opacity.combined(with: .scale(scale: HUD.openScale, anchor: .top)))
                    .animation(reduceMotion
                               ? .easeOut(duration: HUD.microFade)
                               : .spring(response: 0.34, dampingFraction: 0.9),
                               value: isCollapsed)
            }
        }
        .frame(width: currentSize.width, height: currentSize.height, alignment: .top)
        .background {
            if reduceTransparency || contrast == .increased || isCollapsed {
                Color.black
            } else {
                LinearGradient(colors: [HUD.surfaceTop, HUD.surfaceBottom], startPoint: .top, endPoint: .bottom)
            }
        }
        .clipShape(backdrop)
        // Collapsed, the panel must be part of the bezel. An edge line there would draw it.
        .overlay { if !isCollapsed { backdrop.stroke(HUD.edgeStroke, lineWidth: DesignTokens.Size.hairline) } }
        .shadow(
            color: isCollapsed ? .clear : HUD.shadow,
            radius: isCollapsed ? 0 : HUD.shadowRadius,
            y: isCollapsed ? 0 : HUD.shadowOffset
        )
        .overlay {
            if !isCollapsed {
                ZStack(alignment: .bottom) {
                    HStack(spacing: 0) {
                        ResizeEdge(model: model, edge: .left, resizeEnded: actions.resizeEnded)
                        Spacer(minLength: 0)
                        ResizeEdge(model: model, edge: .right, resizeEnded: actions.resizeEnded)
                    }
                    // On the visible side edges of the card: inside the top shoulders and above
                    // the bottom corners.
                    .padding(.horizontal, HUD.openShoulder - HUD.edgeHandle / 2)
                    .padding(.bottom, HUD.openCorner)
                    ResizeEdge(model: model, edge: .bottom, resizeEnded: actions.resizeEnded)
                        .padding(.horizontal, HUD.openShoulder + HUD.openCorner)
                }
                .padding(.top, model.notchTopInset)
            }
        }
        .animation(geometryAnimation, value: isCollapsed)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .environment(\.colorScheme, .dark)
    }

    /// The worst reading, so a Mac with no notch shows usage without a hover.
    private var pill: some View {
        HStack(spacing: HUD.valueSpacing) {
            Image(systemName: "gauge.with.needle")
            if let reading = model.pill {
                Text(verbatim: "\(reading.usedPct)%").monospacedDigit()
            }
        }
        .font(HUD.pill)
        .foregroundStyle(model.pill.map { $0.severity.hudColor ?? HUD.calm } ?? HUD.textSecondary)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(model.pill.map { "Usage, \($0.usedPct) percent used" } ?? "Usage")
    }
}

/// One edge of the open card. The card stays centered, so a side drag moves both side edges and
/// the width changes by two times the pointer move. The bottom edge sets a height limit; a list
/// that is longer than the limit scrolls.
private struct ResizeEdge: View {
    enum Edge { case left, right, bottom }

    let model: UsageHUDModel
    let edge: Edge
    let resizeEnded: () -> Void
    @State private var startValue: CGFloat?
    /// SwiftUI resets this when the system cancels the drag, which calls no `onEnded`.
    @GestureState private var dragging = false
    @State private var cursorPushed = false

    private var isBottom: Bool { edge == .bottom }
    private var currentValue: CGFloat { isBottom ? model.expandedContentHeight : model.panelWidth }

    var body: some View {
        Color.clear
            .frame(width: isBottom ? nil : HUD.edgeHandle, height: isBottom ? HUD.bottomEdgeHandle : nil)
            .contentShape(Rectangle())
            .onHover { setCursor($0) }
            // Esc or a new window can remove the edge during a drag, and then `onEnded` never runs.
            .onDisappear {
                setCursor(false)
                if startValue != nil { endDrag() }
            }
            .onChange(of: dragging) { _, active in
                if !active, startValue != nil { endDrag() }
            }
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .updating($dragging) { _, state, _ in state = true }
                    .onChanged { value in
                        let start = startValue ?? currentValue
                        if startValue == nil {
                            startValue = start
                            model.isResizing = true
                        }
                        switch edge {
                        case .left: model.resizePanel(to: start - 2 * value.translation.width)
                        case .right: model.resizePanel(to: start + 2 * value.translation.width)
                        case .bottom: model.resizePanelHeight(to: start + value.translation.height)
                        }
                    }
                    .onEnded { _ in if startValue != nil { endDrag() } }
            )
            .help("Drag to resize")
            .accessibilityElement()
            .accessibilityLabel(isBottom ? "Panel height" : "Panel width")
            .accessibilityValue("\(Int(currentValue)) points")
            .accessibilityAdjustableAction { direction in
                let step: CGFloat
                switch direction {
                case .increment: step = 40
                case .decrement: step = -40
                @unknown default: return
                }
                model.isResizing = true
                if isBottom { model.resizePanelHeight(to: currentValue + step) } else { model.resizePanel(to: currentValue + step) }
                endDrag()
            }
            // Last, so it hides the element built above. One width element is enough.
            .accessibilityHidden(edge == .left)
    }

    private func endDrag() {
        startValue = nil
        if isBottom { model.savePanelHeightLimit() } else { model.savePanelWidth() }
        model.isResizing = false
        resizeEnded()
    }

    private func setCursor(_ on: Bool) {
        guard on != cursorPushed else { return }
        cursorPushed = on
        if on { (isBottom ? NSCursor.resizeUpDown : NSCursor.resizeLeftRight).push() } else { NSCursor.pop() }
    }
}
