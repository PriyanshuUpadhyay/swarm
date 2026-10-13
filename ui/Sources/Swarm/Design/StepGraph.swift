import SwiftUI

struct GraphEdge: Identifiable {
    let from: String
    let to: String
    var dashed = false
    var stale = false
    var id: String { from + ">" + to }
}

/// Presentation only. Callers supply their own node state and selection action.
struct StepGraph<Content: View>: View {
    @Environment(\.designTokens) private var tokens
    let layers: [[String]]
    let edges: [GraphEdge]
    @ViewBuilder let node: (String) -> Content

    var body: some View {
        let layerOf = Dictionary(uniqueKeysWithValues: layers.enumerated().flatMap { index, ids in ids.map { ($0, index) } })
        VStack(spacing: tokens.spacing.xl) {
            ForEach(Array(layers.enumerated()), id: \.element) { _, ids in
                HStack(alignment: .top, spacing: tokens.spacing.s) {
                    ForEach(ids, id: \.self) { id in
                        node(id).anchorPreference(key: NodeBounds.self, value: .bounds) { [id: $0] }
                    }
                }
            }
        }
        // Long edges use the left gutter, so they do not cross a node.
        .padding(.leading, tokens.spacing.l)
        .backgroundPreferenceValue(NodeBounds.self) { bounds in
            GeometryReader { proxy in
                ForEach(edges) { edge in
                    if let from = bounds[edge.from].map({ proxy[$0] }), let to = bounds[edge.to].map({ proxy[$0] }) {
                        path(from: from, to: to, long: (layerOf[edge.to] ?? 0) - (layerOf[edge.from] ?? 0) > 1)
                            .stroke(edge.stale ? Color.orange : Color.secondary,
                                    style: StrokeStyle(lineWidth: DesignTokens.Size.hairline,
                                                       dash: edge.dashed || edge.stale ? [tokens.spacing.xs, tokens.spacing.xs] : []))
                    }
                }
            }
            .accessibilityHidden(true)
        }
    }

    private func path(from: CGRect, to: CGRect, long: Bool) -> Path {
        Path { path in
            if long {
                let gutter = min(from.minX, to.minX) - tokens.spacing.s
                path.move(to: CGPoint(x: from.minX, y: from.midY))
                path.addLine(to: CGPoint(x: gutter, y: from.midY))
                path.addLine(to: CGPoint(x: gutter, y: to.midY))
                path.addLine(to: CGPoint(x: to.minX, y: to.midY))
            } else {
                path.move(to: CGPoint(x: from.midX, y: from.maxY))
                path.addLine(to: CGPoint(x: to.midX, y: to.minY))
            }
        }
    }
}

private struct NodeBounds: PreferenceKey {
    static let defaultValue: [String: Anchor<CGRect>] = [:]
    static func reduce(value: inout [String: Anchor<CGRect>], nextValue: () -> [String: Anchor<CGRect>]) {
        value.merge(nextValue()) { first, _ in first }
    }
}

struct GraphNodeButton<Content: View>: View {
    @Environment(\.designTokens) private var tokens
    var selected = false
    var invalid = false
    let action: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        Button(action: action) {
            content()
                .padding(tokens.spacing.s)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
                .overlay {
                    RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                        .strokeBorder(invalid ? Color.red : selected ? Color.accentColor : Color.secondary.opacity(DesignTokens.endedPaneOpacity),
                                      lineWidth: DesignTokens.Size.hairline)
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
