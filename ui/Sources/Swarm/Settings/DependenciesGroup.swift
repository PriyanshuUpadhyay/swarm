import SwiftUI
import SwarmCore

struct DependenciesGroup: View {
    @Environment(\.designTokens) private var tokens
    let rows: [DependencyRow]

    var body: some View {
        GroupBox("Dependencies") {
            VStack(alignment: .leading, spacing: tokens.spacing.s) {
                ForEach(rows) { row in
                    HStack(alignment: .top, spacing: tokens.spacing.s) {
                        Image(systemName: row.path == nil ? "xmark.circle" : "checkmark.circle")
                            .foregroundStyle(row.path == nil ? .red : .green)
                            .accessibilityLabel(row.path == nil ? "Missing" : "Installed")
                        VStack(alignment: .leading, spacing: tokens.spacing.xxs) {
                            Text(row.name).font(.headline)
                            if let path = row.path { Text(verbatim: path).foregroundStyle(.secondary) }
                            Text(row.installHint).font(.caption).foregroundStyle(.secondary)
                        }
                        .textSelection(.enabled)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(tokens.spacing.s)
        }
    }
}
