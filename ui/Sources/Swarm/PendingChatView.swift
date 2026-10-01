import SwiftUI
import SwarmCore

/// The chat area of a chat the app is starting, or of a start that failed (ADR 0035).
struct PendingChatView: View {
    let chat: PendingChat
    let retry: () -> Void
    let close: () -> Void

    var body: some View {
        VStack(spacing: DesignTokens.Spacing.m) {
            if case .failed(let failure) = chat.state {
                Text("Could not start the chat").font(.title3.weight(.semibold))
                Text(verbatim: failure.message)
                    .font(.callout).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center).textSelection(.enabled)
                if failure.missingCLI {
                    Text("Install Claude Code or Codex, then Retry.").font(.callout)
                }
                HStack {
                    Button("Close", action: close)
                    Button("Retry", action: retry).keyboardShortcut(.defaultAction)
                }
            } else {
                DelayedProgress("Starting chat…")
                Text(verbatim: chat.directory)
                    .font(.caption).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
