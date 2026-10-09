import SwiftUI
import SwarmCore

struct AdvancedSettingsPage: View {
    @Binding var showRawData: Bool
    @Binding var performanceLogging: Bool
    let resetDeclinedPrompts: () -> Void
    let dataHome: String?
    let revealDataHome: () -> Void
    let helperVersion: String
    let drift: PathSwarmDrift?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xl) {
                Text("Advanced").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                    Toggle("Show Raw Data", isOn: $showRawData)
                    Toggle("Performance Logging", isOn: $performanceLogging)
                    Button("Reset declined prompts", action: resetDeclinedPrompts)
                }
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                    Text("Data home").font(.headline)
                    Text(verbatim: dataHome ?? "SWARM_HOME is set but empty")
                        .font(DesignTokens.mono).textSelection(.enabled)
                    Button("Reveal in Finder", action: revealDataHome).disabled(dataHome == nil)
                }
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                    Text("Helper version").font(.headline)
                    Text(verbatim: helperVersion).font(DesignTokens.mono).textSelection(.enabled)
                    if let drift {
                        Label("Terminal uses a different swarm build.", systemImage: "exclamationmark.triangle")
                        Text(verbatim: "\(drift.pathLine) at \(drift.path)")
                            .font(DesignTokens.mono).textSelection(.enabled)
                        Text(verbatim: drift.fixCommand).font(DesignTokens.mono).textSelection(.enabled)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(DesignTokens.Spacing.xl)
        }
    }
}
