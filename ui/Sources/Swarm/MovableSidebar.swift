import SwiftUI
import SwarmCore

struct MovableSidebar<Sidebar: View, Content: View>: View {
    let visible: Bool
    let onRight: Bool
    let minimumContentWidth: CGFloat
    @Binding var width: Double
    @ViewBuilder let sidebar: () -> Sidebar
    @ViewBuilder let content: () -> Content
    @State private var dragStart: Double?

    var body: some View {
        GeometryReader { geometry in
            let range = SidebarWidth.range
            let availableWidth = max(range.lowerBound, geometry.size.width - minimumContentWidth - 1)
            let actualWidth = min(max(width, range.lowerBound), min(range.upperBound, availableWidth))
            let inset = visible ? actualWidth + 1 : 0
            ZStack(alignment: onRight ? .trailing : .leading) {
                content()
                    .padding(.leading, onRight ? 0 : inset)
                    .padding(.trailing, onRight ? inset : 0)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                sidebar()
                    .frame(width: actualWidth, height: geometry.size.height)
                    .clipped()
                    .retainedVisibility(visible)
                    .overlay(alignment: onRight ? .leading : .trailing) {
                        if visible {
                            Rectangle().fill(.separator).frame(width: DesignTokens.Size.hairline)
                                .frame(width: DesignTokens.Size.dragHandle).contentShape(Rectangle())
                                .gesture(DragGesture().onChanged { value in
                                    if dragStart == nil { dragStart = actualWidth }
                                    width = min(range.upperBound, max(range.lowerBound,
                                        (dragStart ?? actualWidth) + value.translation.width * (onRight ? -1 : 1)))
                                }.onEnded { _ in dragStart = nil })
                                .accessibilityLabel("Sidebar width")
                                .accessibilityAdjustableAction { direction in
                                    width = min(range.upperBound, max(range.lowerBound,
                                        actualWidth + (direction == .increment ? SidebarWidth.step : -SidebarWidth.step)))
                                }
                        }
                    }
            }
        }
        .frame(minWidth: minimumContentWidth + (visible ? SidebarWidth.range.lowerBound + 1 : 0), minHeight: 420)
    }
}
