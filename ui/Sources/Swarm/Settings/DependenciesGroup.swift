import SwiftUI
import SwarmCore

struct DependenciesGroup: View {
    let rows: [DependencyRow]

    var body: some View {
        GroupBox("Dependencies") {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                ForEach(rows) { row in
                    HStack(alignment: .top, spacing: DesignTokens.Spacing.s) {
                        Image(systemName: row.path == nil ? "xmark.circle" : "checkmark.circle")
                            .foregroundStyle(row.path == nil ? .red : .green)
                            .accessibilityLabel(row.path == nil ? "Missing" : "Installed")
                        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                            Text(row.name).font(.headline)
                            if let path = row.path { Text(verbatim: path).foregroundStyle(.secondary) }
                            Text(row.installHint).font(.caption).foregroundStyle(.secondary)
                        }
                        .textSelection(.enabled)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(DesignTokens.Spacing.s)
        }
    }
}
