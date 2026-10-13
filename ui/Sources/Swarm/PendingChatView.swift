import SwiftUI
import SwarmCore

/// The chat area of a chat the app is starting, or of a start that failed (ADR 0035).
struct PendingChatView: View {
    @Environment(\.designTokens) private var tokens
    let chat: PendingChat
    let retry: () -> Void
    let close: () -> Void

    var body: some View {
        VStack(spacing: tokens.spacing.m) {
            if case .failed(let failure) = chat.state {
                Text("Could not start the chat").font(.title3.weight(.semibold))
                // swarm's whole stderr can be long; it scrolls, so Retry and Close stay in view.
                ScrollView {
                    Text(verbatim: failure.message)
                        .font(.callout).foregroundStyle(.secondary)
                        .multilineTextAlignment(.center).textSelection(.enabled)
                        .frame(maxWidth: .infinity)
                }
                .frame(maxHeight: DesignTokens.Size.pickerList)
                .fixedSize(horizontal: false, vertical: true)
                if failure.missingCLI {
                    Text("Install Claude Code or Codex, then Retry.").font(.callout)
                }
                HStack {
                    Button("Close", action: close)
                    Button("Retry", action: retry).keyboardShortcut(.defaultAction)
                }
            } else if case .closing = chat.state {
                DelayedProgress("Closing…")
            } else {
                DelayedProgress("Starting chat…")
                Text(verbatim: chat.directory)
                    .font(.caption).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
        }
        .padding(tokens.spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
