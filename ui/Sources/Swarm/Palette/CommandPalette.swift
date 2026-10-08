import SwiftUI
import SwarmCore

/// The command palette: one search field over grouped results. ↑ and ↓ move, Return runs, Esc
/// closes. Keys open and close it, so it appears and goes with no animation.
struct CommandPalette: View {
    let items: [PaletteItem]
    let run: (PaletteItem) -> Void
    let close: () -> Void

    @State private var query = ""
    @State private var selectedID: String?
    @FocusState private var fieldFocused: Bool

    /// With a query, results group as Actions, Workspaces, Chats, Agents; with none, recent items
    /// come first under Recent, then the actions. The rank order holds within a section.
    private var sections: [(title: String, items: [PaletteItem])] {
        let ranked = PaletteSearch.rank(items: items, query: query)
        if query.trimmingCharacters(in: .whitespaces).isEmpty {
            return [
                ("Recent", ranked.filter { $0.recency != nil }),
                (PaletteItem.Group.action.title, ranked.filter { $0.recency == nil }),
            ].filter { !$0.items.isEmpty }
        }
        return PaletteItem.Group.allCases.map { group in (group.title, ranked.filter { $0.group == group }) }
            .filter { !$0.items.isEmpty }
    }

    var body: some View {
        let sections = sections
        let results = sections.flatMap(\.items)
        VStack(spacing: 0) {
            TextField("Search actions, workspaces, chats, and agents", text: $query)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($fieldFocused)
                .padding(DesignTokens.Spacing.l)
                .onSubmit { runSelected(in: results) }
                .onKeyPress(.upArrow) { move(-1, in: results) }
                .onKeyPress(.downArrow) { move(1, in: results) }
                .onKeyPress(.escape) {
                    close()
                    return .handled
                }
            if !results.isEmpty {
                Divider()
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(sections, id: \.title) { section in
                                let rows = section.items
                                if !rows.isEmpty {
                                    Text(section.title)
                                        .font(.caption.weight(.semibold))
                                        .foregroundStyle(.secondary)
                                        .padding(.horizontal, DesignTokens.Spacing.l)
                                        .padding(.top, DesignTokens.Spacing.s)
                                        .padding(.bottom, DesignTokens.Spacing.xs)
                                        .accessibilityAddTraits(.isHeader)
                                    ForEach(rows) { item in
                                        PaletteRow(item: item, selected: item.id == currentID(in: results))
                                            .id(item.id)
                                            .onTapGesture { if item.disabledReason == nil { run(item) } }
                                            .opacity(item.disabledReason == nil ? 1 : DesignTokens.endedPaneOpacity)
                                            .help(item.disabledReason ?? item.title)
                                    }
                                }
                            }
                        }
                        .padding(.bottom, DesignTokens.Spacing.s)
                    }
                    .frame(maxHeight: DesignTokens.Size.paletteResults)
                    .onChange(of: selectedID) { _, id in
                        if let id { proxy.scrollTo(id) }
                    }
                }
            } else if !query.isEmpty {
                Divider()
                Text("No matches")
                    .foregroundStyle(.secondary)
                    .padding(DesignTokens.Spacing.l)
            }
        }
        .frame(width: DesignTokens.Size.palette)
        .chromeSurface(in: RoundedRectangle(cornerRadius: DesignTokens.Radius.panel, style: .continuous))
        .onAppear { fieldFocused = true }
        .onChange(of: query) { selectedID = nil }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Command palette")
    }

    /// The highlighted row: the one ↑ or ↓ chose, else the first result.
    private func currentID(in results: [PaletteItem]) -> String? {
        results.contains { $0.id == selectedID } ? selectedID : results.first?.id
    }

    private func move(_ delta: Int, in results: [PaletteItem]) -> KeyPress.Result {
        guard !results.isEmpty else { return .handled }
        let index = results.firstIndex { $0.id == currentID(in: results) } ?? 0
        selectedID = results[min(max(index + delta, 0), results.count - 1)].id
        return .handled
    }

    private func runSelected(in results: [PaletteItem]) {
        guard let id = currentID(in: results), let item = results.first(where: { $0.id == id }) else { return }
        if item.disabledReason == nil { run(item) }
    }
}

private struct PaletteRow: View {
    let item: PaletteItem
    let selected: Bool

    var body: some View {
        HStack(spacing: DesignTokens.Spacing.s) {
            Group {
                if let status = item.status { StatusGlyph(status: status) }
            }
            .frame(width: DesignTokens.Size.glyphSlot)
            Text(item.title).lineLimit(1)
            if let subtitle = item.subtitle {
                Text(subtitle).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: DesignTokens.Spacing.s)
            if let shortcut = item.shortcut {
                Text(shortcut).font(.callout.monospaced()).foregroundStyle(.secondary)
            }
        }
        .frame(minHeight: DesignTokens.Size.row)
        .padding(.horizontal, DesignTokens.Spacing.m)
        .background(selected ? DesignTokens.selectionAccentFill : .clear,
                    in: .rect(cornerRadius: DesignTokens.Radius.control))
        .padding(.horizontal, DesignTokens.Spacing.xs)
        .contentShape(Rectangle())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([item.title, item.subtitle, item.disabledReason, item.status.map(StatusGlyph.title)].compactMap { $0 }.joined(separator: ", "))
        .accessibilityValue(item.shortcut ?? "")
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }
}
