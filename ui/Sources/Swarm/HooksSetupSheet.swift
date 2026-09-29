import SwiftUI
import SwarmCore

extension Notification.Name {
    /// The app menu's "Set Up Agent Hooks…" asks the window to show the sheet.
    static let showHooksSetup = Notification.Name("SwarmShowHooksSetup")
}

/// The one consent question before swarm changes the owner's Codex and AGY config (ADR 0029).
/// "Not now" writes nothing; the app menu offers the same sheet later.
struct HooksSetupSheet: View {
    let setUp: () async throws -> Void
    let notNow: () -> Void
    let done: () -> Void

    @State private var working = false
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            Text("Let swarm set up its own hooks for Codex and AGY?")
                .font(.headline)
            Text("Agents then report their chat and state to the app, so their columns show a chat and their questions. Swarm trusts only its own Codex hooks and adds its own AGY hooks; your other hooks stay as they are. Claude needs no step.")
                .fixedSize(horizontal: false, vertical: true)
            if let failure {
                Text(failure).font(.caption).foregroundStyle(.red)
            }
            HStack {
                Spacer()
                Button("Not now", action: notNow)
                    .keyboardShortcut(.cancelAction)
                Button {
                    working = true
                    failure = nil
                    Task {
                        do {
                            try await setUp()
                            done()
                        } catch {
                            failure = (error as? SwarmProfileError)?.message ?? String(describing: error)
                        }
                        working = false
                    }
                } label: {
                    if working { ProgressView().controlSize(.small) } else { Text("Set up") }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(working)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet)
    }
}
