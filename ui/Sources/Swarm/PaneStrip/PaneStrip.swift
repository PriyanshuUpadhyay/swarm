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

/// The chat page, then columns of agent panes that scroll in from the right, with no snap (ADR 0030).
/// Focus and zoom change with no animation, because keys drive them. The owner drags a column's
/// trailing edge to size every column and the line in a two-pane column to split it (ADR 0026).
struct PaneStrip<Chat: View, Pane: View>: View {
    let cells: [PaneCell]
    let focusedID: String?
    let zoomedID: String?
    /// The pane a key moved focus to; nil shows the chat page. The strip scrolls to it.
    let revealID: String?
    /// Changes on every reveal, so a repeated reveal of the chat still scrolls.
    let revealCount: Int
    /// Saved splits belong to this chat, so two chats with the same agent id keep their own.
    let splitScope: String
    let onFocus: (String) -> Void
    let onZoom: (String?) -> Void
    let onDismiss: ([String]) -> Void
    let readOnlyReason: String?
    let onStop: (String) -> Void
    let onClose: (String) -> Void
    @ViewBuilder let chat: () -> Chat
    @ViewBuilder let pane: (PaneCell) -> Pane

    /// nil is the default width, a third of the main area.
    @AppStorage("paneColumnWidth") private var storedColumnWidth: Double?
    @AppStorage("chatPageWidth") private var storedChatWidth: Double?
    @AppStorage("paneColumnSplits") private var storedSplits = ""
    @State private var main = CGSize.zero
    @State private var scrollPosition = ScrollPosition()
    @State private var scrollOffset = ScrollOffset()
    @State private var widthDragStart: (width: CGFloat, offset: CGFloat)?
    @State private var splitDragStart: Double?
    @State private var chatDragStart: CGFloat?

    var body: some View {
        let hasPanes = !cells.isEmpty
        let hasFinished = cells.contains(where: \.ended)
        let zoomed = cells.first { $0.id == zoomedID }
        let preferredWidth = storedColumnWidth.map { CGFloat($0) }
        let splits = PaneStripLayout.splits(from: storedSplits, scope: splitScope)
        ZStack {
            // The chat stays in the scroll view with no panes, so it keeps its identity and state.
            ScrollViewReader { proxy in
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 0) {
                        chat()
                            .padding(.trailing, hasPanes ? DesignTokens.Size.dragHandle : 0)
                            .containerRelativeFrame([.horizontal, .vertical]) { length, axis in
                                axis == .horizontal && hasPanes
                                    ? PaneStripLayout.widths(main: length, chat: storedChatWidth.map { CGFloat($0) }).chat
                                    : length
                            }
                            .overlay(alignment: .trailing) {
                                if hasPanes { chatWidthHandle }
                            }
                            .id(Self.chatID)
                        ForEach(Array(columns.enumerated()), id: \.element[0].id) { index, column in
                            columnView(column, split: PaneStripLayout.split(preferred: splits[column[0].id]),
                                       splits: splits, zoomedID: zoomed?.id)
                            .containerRelativeFrame([.horizontal, .vertical]) { length, axis in
                                axis == .horizontal
                                    ? PaneStripLayout.columnWidth(main: length, preferred: preferredWidth) : length
                            }
                            .overlay(alignment: .trailing) { widthHandle(column: index) }
                        }
                    }
                }
                .scrollPosition($scrollPosition)
                .onScrollGeometryChange(for: CGFloat.self) { $0.contentOffset.x } action: { _, x in
                    scrollOffset.x = x
                }
                .onGeometryChange(for: CGSize.self) { $0.size } action: { main = $0 }
                .scrollDisabled(!hasPanes)
                .scrollIndicators(hasPanes ? .automatic : .hidden)
                .onChange(of: revealCount) {
                    // No animation: a key moved focus here.
                    let column = columns.first { $0.contains { $0.id == revealID } }?.first?.id
                    proxy.scrollTo(column ?? Self.chatID)
                }
            }
            .retainedVisibility(zoomed == nil)
            if let zoomed {
                paneView(zoomed, showsContent: true)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if hasPanes {
                HStack {
                    Spacer()
                    Button("Clear finished") { onDismiss(cells.filter(\.ended).map(\.id)) }
                        .buttonStyle(.borderless)
                        .disabled(!hasFinished)
                        .opacity(hasFinished ? 1 : 0)
                        .accessibilityHidden(!hasFinished)
                }
                .font(.caption)
                .padding(DesignTokens.Spacing.s)
                .background(.bar)
            }
        }
    }

    private static var chatID: String { "pane-strip-chat" }

    private var columnWidth: CGFloat {
        PaneStripLayout.columnWidth(main: main.width, preferred: storedColumnWidth.map { CGFloat($0) })
    }

    @ViewBuilder
    private func columnView(_ column: [PaneCell], split: Double, splits: [String: Double],
                            zoomedID: String?) -> some View {
        if column.count == 2 {
            let topHeight = main.height * split
            VStack(spacing: 0) {
                paneView(column[0], showsContent: column[0].id != zoomedID).frame(height: topHeight)
                paneView(column[1], showsContent: column[1].id != zoomedID)
            }
            .overlay(alignment: .top) {
                ResizeHandle(axis: .vertical, label: "Split of \(column[0].title)") { translation in
                    if splitDragStart == nil { splitDragStart = split }
                    let start = splitDragStart ?? split
                    saveSplit(PaneStripLayout.split(preferred: start + translation / max(main.height, 1)),
                              column: column[0].id, splits: splits)
                } onEnd: {
                    splitDragStart = nil
                } onReset: {
                    saveSplit(nil, column: column[0].id, splits: splits)
                } onAdjust: { step in
                    saveSplit(PaneStripLayout.split(preferred: split + step * 0.05), column: column[0].id,
                              splits: splits)
                }
                .offset(y: topHeight - DesignTokens.Size.dragHandle / 2)
            }
        } else {
            VStack(spacing: 0) {
                ForEach(column) { paneView($0, showsContent: $0.id != zoomedID) }
            }
        }
    }

    private var chatWidth: CGFloat {
        PaneStripLayout.widths(main: main.width, chat: storedChatWidth.map { CGFloat($0) }).chat
    }

    private var chatWidthHandle: some View {
        ResizeHandle(axis: .horizontal, label: "Chat page width") { translation in
            if chatDragStart == nil { chatDragStart = chatWidth }
            resizeChat(to: (chatDragStart ?? chatWidth) + translation)
        } onEnd: {
            chatDragStart = nil
        } onReset: {
            resizeChat(to: nil)
        } onAdjust: { step in
            resizeChat(to: chatWidth + step * 20)
        }
    }

    private func resizeChat(to preferred: CGFloat?) {
        storedChatWidth = preferred.map { Double(PaneStripLayout.widths(main: main.width, chat: $0).chat) }
    }

    private func widthHandle(column index: Int) -> some View {
        ResizeHandle(axis: .horizontal, label: "Agent column width") { translation in
            if widthDragStart == nil { widthDragStart = (columnWidth, scrollOffset.x) }
            let start = widthDragStart ?? (columnWidth, scrollOffset.x)
            resizeColumns(to: start.width + translation, keeping: index, from: start)
        } onEnd: {
            widthDragStart = nil
        } onReset: {
            resizeColumns(to: nil, keeping: index, from: (columnWidth, scrollOffset.x))
        } onAdjust: { step in
            resizeColumns(to: columnWidth + step * 20, keeping: index, from: (columnWidth, scrollOffset.x))
        }
    }

    /// Every column changes width, so the strip scrolls by the change of the columns before this
    /// one. Its leading edge stays put, so the edge tracks the pointer.
    private func resizeColumns(to preferred: CGFloat?, keeping index: Int,
                               from start: (width: CGFloat, offset: CGFloat)) {
        let width = PaneStripLayout.columnWidth(main: main.width, preferred: preferred)
        storedColumnWidth = preferred.map { _ in Double(width) }
        scrollPosition.scrollTo(x: start.offset + CGFloat(index) * (width - start.width))
    }

    private func saveSplit(_ split: Double?, column id: String, splits: [String: Double]) {
        var splits = splits
        splits[id] = split
        storedSplits = PaneStripLayout.text(saving: splits, scope: splitScope,
                                            keeping: Set(columns.map { $0[0].id }), in: storedSplits)
    }

    private var columns: [[PaneCell]] {
        PaneStripLayout.columns(count: cells.count).map { $0.map { cells[$0] } }
    }

    private func paneView(_ cell: PaneCell, showsContent: Bool) -> some View {
        PaneView(
            cell: cell, focused: cell.id == focusedID, zoomed: cell.id == zoomedID,
            onFocus: { onFocus(cell.id) },
            onZoom: { onZoom(cell.id == zoomedID ? nil : cell.id) },
            onDismiss: { onDismiss([cell.id]) },
            readOnlyReason: readOnlyReason,
            onStop: { onStop(cell.id) }, onClose: { onClose(cell.id) }
        ) {
            // A column shows in one place only; the zoomed copy owns it.
            if showsContent { pane(cell) } else { Color.clear }
        }
    }
}

/// Scroll offset read only when a drag starts, so scrolling does not redraw the strip.
private final class ScrollOffset {
    var x: CGFloat = 0
}

/// A clear grip on a pane edge. It takes no focus, so keys stay with the columns.
private struct ResizeHandle: View {
    let axis: Axis
    let label: String
    /// The pointer's travel since the drag began, along the axis.
    let onDrag: (CGFloat) -> Void
    let onEnd: () -> Void
    let onReset: () -> Void
    /// +1 or -1 from VoiceOver.
    let onAdjust: (CGFloat) -> Void

    var body: some View {
        Color.clear
            .frame(width: axis == .horizontal ? DesignTokens.Size.dragHandle : nil,
                   height: axis == .vertical ? DesignTokens.Size.dragHandle : nil)
            .contentShape(Rectangle())
            .pointerStyle(axis == .horizontal ? .columnResize : .rowResize)
            // Global space: the grip moves as it resizes, and the drag must follow the pointer.
            .gesture(DragGesture(minimumDistance: 1, coordinateSpace: .global)
                .onChanged { onDrag(axis == .horizontal ? $0.translation.width : $0.translation.height) }
                .onEnded { _ in onEnd() })
            .onTapGesture(count: 2, perform: onReset)
            .help("Drag to resize; double-click to reset")
            .accessibilityElement()
            .accessibilityLabel(label)
            .accessibilityAdjustableAction { onAdjust($0 == .increment ? 1 : -1) }
    }
}

private struct PaneView<Content: View>: View {
    let cell: PaneCell
    let focused: Bool
    let zoomed: Bool
    let onFocus: () -> Void
    let onZoom: () -> Void
    let onDismiss: () -> Void
    let readOnlyReason: String?
    let onStop: () -> Void
    let onClose: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            header
            content()
                .opacity(cell.ended ? DesignTokens.endedPaneOpacity : 1)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay {
            Rectangle()
                .strokeBorder(focused ? Color.accentColor : Color(nsColor: .separatorColor),
                              lineWidth: focused ? DesignTokens.Size.focusRing : DesignTokens.Size.hairline)
                .allowsHitTesting(false)
        }
    }

    private struct HeaderAction: Identifiable {
        var id: String { title }
        let title: String
        let symbol: String
        let label: String
        var help: String?
        var disabled = false
        let perform: () -> Void
    }

    private var headerActions: [HeaderAction] {
        [
            HeaderAction(title: "Stop", symbol: "stop.fill", label: "Stop " + cell.title,
                         help: readOnlyReason, disabled: readOnlyReason != nil || !cell.status.isMidTurn,
                         perform: onStop),
            HeaderAction(title: "Close agent…", symbol: "power", label: "Close agent " + cell.title,
                         help: readOnlyReason ?? "Close agent", disabled: readOnlyReason != nil || cell.ended,
                         perform: onClose),
            HeaderAction(title: "Copy id", symbol: "doc.on.doc", label: "Copy id of " + cell.title,
                         perform: { AppClipboard.copy(cell.id) }),
            HeaderAction(title: "Copy attach command", symbol: "terminal", label: "Copy attach command for " + cell.title,
                         perform: { AppClipboard.copy(SwarmAgentID(cell.id).attachCommand) }),
        ]
    }

    private var header: some View {
        HStack(spacing: DesignTokens.Spacing.s) {
            StatusGlyph(status: cell.status)
            Text(cell.title).fontWeight(.semibold)
            Text("\(cell.role) · \(cell.model)").foregroundStyle(.secondary)
            Spacer(minLength: 4)
            ForEach(headerActions) { action in
                Button(action: action.perform) { Image(systemName: action.symbol) }
                    .focusable(false)
                    .disabled(action.disabled)
                    .help(action.help ?? action.title)
                    .accessibilityLabel(action.label)
            }
            Button(action: onZoom) {
                Image(systemName: zoomed
                      ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
            }
            .help(zoomed ? "Return to the strip" : "Zoom pane")
            // ⌘↩ zooms from the keyboard; this button must not be the window's first key view.
            .focusable(false)
            .accessibilityLabel(zoomed ? "Unzoom \(cell.title)" : "Zoom \(cell.title)")
            if cell.ended {
                Button(action: onDismiss) { Image(systemName: "xmark") }
                    .help("Dismiss finished agent")
                    .focusable(false)
                    .accessibilityLabel("Dismiss \(cell.title)")
            }
        }
        .buttonStyle(.borderless)
        .font(.caption)
        .lineLimit(1)
        .padding(.horizontal, DesignTokens.Spacing.s)
        .frame(height: DesignTokens.Size.paneHeader)
        .foregroundStyle(focused ? .primary : .secondary)
        .background(Color(nsColor: .windowBackgroundColor))
        .contentShape(Rectangle())
        // One header focus target gives the keyboard access to its menu.
        .focusable()
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Controls for \(cell.id)")
        .contextMenu {
            ForEach(headerActions) { action in
                Button(action.title, action: action.perform)
                    .disabled(action.disabled)
            }
        }
        .onTapGesture(perform: onFocus)
    }
}
