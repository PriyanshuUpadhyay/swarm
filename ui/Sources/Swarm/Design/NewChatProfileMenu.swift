import SwiftUI
import SwarmCore

struct NewChatProfileMenu<Label: View>: View {
    let rows: [NewChatMenu.Row]
    let refresh: () -> Void
    let start: (String) -> Void
    let primaryAction: () -> Void
    @ViewBuilder let label: () -> Label

    var body: some View {
        Menu {
            Text("New chat as…")
                // Menu content appearance is a best-effort refresh; launch also loads the rows.
                .onAppear(perform: refresh)
            if rows.isEmpty { Text("No profiles available") }
            ForEach(rows) { row in
                Button { start(row.name) } label: {
                    Text("\(Text(name(row)))\(Text(row.caption.map { "\n" + $0 } ?? "").font(.caption))")
                }
                .disabled(!row.isEnabled)
            }
        } label: {
            label()
        } primaryAction: {
            primaryAction()
        }
    }

    private func name(_ row: NewChatMenu.Row) -> String {
        row.name + (row.isDefault ? " (one-click default)" : "")
    }
}
