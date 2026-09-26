import SwiftUI

enum WorkspaceSidebarMode: String, CaseIterable {
    case workspaces = "Workspaces", files = "Files", changes = "Changes", pullRequest = "PR", usage = "Usage"

    var symbol: String {
        switch self {
        case .workspaces: "square.stack.3d.up"
        case .files: "folder"
        case .changes: "arrow.triangle.branch"
        case .pullRequest: "arrow.triangle.pull"
        case .usage: "chart.pie"
        }
    }
}

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
            let actualWidth = min(max(width, 230), min(440, max(230, geometry.size.width - minimumContentWidth - 1)))
            let inset = visible ? actualWidth + 1 : 0
            ZStack(alignment: onRight ? .trailing : .leading) {
                content()
                    .padding(.leading, onRight ? 0 : inset)
                    .padding(.trailing, onRight ? inset : 0)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                sidebar()
                    .frame(width: actualWidth, height: geometry.size.height)
                    .clipped()
                    .opacity(visible ? 1 : 0)
                    .allowsHitTesting(visible)
                    .accessibilityHidden(!visible)
                    .overlay(alignment: onRight ? .leading : .trailing) {
                        if visible {
                            Rectangle().fill(.separator).frame(width: 1)
                                .frame(width: 7).contentShape(Rectangle())
                                .gesture(DragGesture().onChanged { value in
                                    if dragStart == nil { dragStart = actualWidth }
                                    width = min(440, max(230, (dragStart ?? actualWidth) + value.translation.width * (onRight ? -1 : 1)))
                                }.onEnded { _ in dragStart = nil })
                                .accessibilityLabel("Sidebar width")
                                .accessibilityAdjustableAction { direction in
                                    width = min(440, max(230, actualWidth + (direction == .increment ? 20 : -20)))
                                }
                        }
                    }
            }
        }
        .frame(minWidth: minimumContentWidth + (visible ? 231 : 0), minHeight: 420)
    }
}
