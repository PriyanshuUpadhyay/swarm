import SwiftUI
import SwarmCore

struct RecentlyClosedSheet: View {
    @Environment(\.designTokens) private var tokens
    let chats: [RecentlyClosedChat]
    let notice: String?
    let loading: Bool
    let restore: (SwarmProjectSession) async -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var selection: SwarmSessionID?
    @State private var restoring = false

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.l) {
            Text("Recently closed").font(.title2)
            if loading {
                ProgressView("Loading closed chats…")
            } else if chats.isEmpty {
                Text("No archived chats").foregroundStyle(.secondary)
            } else {
                List(chats, selection: $selection) { row in
                    Button {
                        selection = row.id
                        restoreSelected()
                    } label: {
                        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                            Text(row.title).lineLimit(1)
                            Text("\(row.workspace) · \(row.age)").font(.caption).foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .tag(row.id)
                    .disabled(restoring)
                }
            }
            if let notice { Text(verbatim: notice).font(.caption).foregroundStyle(.secondary) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Open") { restoreSelected() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(loading || selection == nil || restoring)
            }
        }
        .padding(tokens.spacing.l)
        .frame(width: DesignTokens.Size.sheet, height: DesignTokens.Size.sheetHeight)
        .onChange(of: chats.map(\.id), initial: true) {
            if !chats.contains(where: { $0.id == selection }) { selection = chats.first?.id }
        }
    }

    private func restoreSelected() {
        guard !restoring, let row = chats.first(where: { $0.id == selection }) else { return }
        restoring = true
        Task {
            await restore(row.chat)
            restoring = false
        }
    }
}
