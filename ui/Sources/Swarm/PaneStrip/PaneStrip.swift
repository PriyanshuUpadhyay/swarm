import SwiftUI
import SwarmCore

/// One agent pane in the strip, as plain values.
struct PaneCell: Identifiable, Equatable {
    let id: String
    let title: String
    let role: String
    let model: String
    let status: AgentStatus

    var ended: Bool { status == .ended }
}

/// The chat page, then columns of agent panes that scroll in from the right (ADR 0022).
/// Focus and zoom change with no animation, because keys drive them.
struct PaneStrip<Chat: View, Pane: View>: View {
    let cells: [PaneCell]
    let focusedID: String?
    let zoomedID: String?
    /// The pane a key moved focus to; nil shows the chat page. The strip scrolls to it.
    let revealID: String?
    let onFocus: (String) -> Void
    let onZoom: (String?) -> Void
    let onReconnect: (String) -> Void
    @ViewBuilder let chat: () -> Chat
    @ViewBuilder let pane: (PaneCell) -> Pane

    var body: some View {
        let hasPanes = !cells.isEmpty
        let zoomed = cells.first { $0.id == zoomedID }
        ZStack {
            // The chat stays in the scroll view with no panes, so it keeps its identity and state.
            ScrollViewReader { proxy in
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 0) {
                        chat()
                            .containerRelativeFrame([.horizontal, .vertical]) { length, axis in
                                axis == .horizontal && hasPanes ? PaneStripLayout.widths(main: length).chat : length
                            }
                            .id(Self.chatID)
                        ForEach(columns, id: \.[0].id) { column in
                            VStack(spacing: 0) {
                                ForEach(column) { cell in
                                    paneView(cell, showsContent: cell.id != zoomed?.id)
                                }
                            }
                            .containerRelativeFrame([.horizontal, .vertical]) { length, axis in
                                axis == .horizontal ? PaneStripLayout.widths(main: length).column : length
                            }
                        }
                    }
                    .scrollTargetLayout()
                }
                .scrollTargetBehavior(.viewAligned)
                .scrollDisabled(!hasPanes)
                .scrollIndicators(hasPanes ? .automatic : .hidden)
                .onChange(of: revealID) { _, id in
                    // No animation: a key moved focus here.
                    let column = columns.first { $0.contains { $0.id == id } }?.first?.id
                    proxy.scrollTo(column ?? Self.chatID)
                }
            }
            .retainedVisibility(zoomed == nil)
            if let zoomed {
                paneView(zoomed, showsContent: true)
            }
        }
    }

    private static var chatID: String { "pane-strip-chat" }

    private var columns: [[PaneCell]] {
        PaneStripLayout.columns(count: cells.count).map { $0.map { cells[$0] } }
    }

    private func paneView(_ cell: PaneCell, showsContent: Bool) -> some View {
        PaneView(
            cell: cell, focused: cell.id == focusedID, zoomed: cell.id == zoomedID,
            onFocus: { onFocus(cell.id) },
            onZoom: { onZoom(cell.id == zoomedID ? nil : cell.id) },
            onReconnect: { onReconnect(cell.id) }
        ) {
            // A terminal shows in one place only; the zoomed copy owns it.
            if showsContent { pane(cell) } else { Color.clear }
        }
    }
}

private struct PaneView<Content: View>: View {
    let cell: PaneCell
    let focused: Bool
    let zoomed: Bool
    let onFocus: () -> Void
    let onZoom: () -> Void
    let onReconnect: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            header
            content()
                .opacity(cell.ended ? 0.6 : 1)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay {
            Rectangle()
                .strokeBorder(focused ? Color.accentColor : Color(nsColor: .separatorColor),
                              lineWidth: focused ? 2 : 1)
                .allowsHitTesting(false)
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            StatusGlyph(status: cell.status)
            Text(cell.title).fontWeight(.semibold)
            Text("\(cell.role) · \(cell.model)").foregroundStyle(.secondary)
            Spacer(minLength: 4)
            if cell.ended {
                Button("Reconnect", action: onReconnect)
            }
            Button(action: onZoom) {
                Image(systemName: zoomed
                      ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
            }
            .help(zoomed ? "Return to the strip" : "Zoom pane")
            .accessibilityLabel(zoomed ? "Unzoom \(cell.title)" : "Zoom \(cell.title)")
        }
        .buttonStyle(.borderless)
        .font(.caption)
        .lineLimit(1)
        .padding(.horizontal, 8)
        .frame(height: 24)
        .foregroundStyle(focused ? .primary : .secondary)
        .background(Color(nsColor: .windowBackgroundColor))
        .contentShape(Rectangle())
        .onTapGesture(perform: onFocus)
    }
}
