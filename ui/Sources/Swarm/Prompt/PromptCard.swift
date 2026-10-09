import SwiftUI
import SwarmCore

/// A question an agent's screen shows, with one button per choice in the CLI's own words
/// (ADR 0029). A child column and the chair page both show it. Nothing is picked for the owner.
struct PromptCard: View {
    @Environment(\.designTokens) private var tokens
    let agent: String
    let prompt: SwarmPrompt
    /// Sends choice `index`; throws with swarm's reason, such as a question that changed.
    let answer: (Int) async throws -> Void
    /// Shown on the chair page, where the column may be scrolled away.
    var showColumn: (() -> Void)? = nil

    @State private var sending: Int?
    @State private var failure: String?
    @FocusState private var focused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.s) {
            HStack {
                Label("\(agent) is waiting for you", systemImage: "questionmark.bubble")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(DesignTokens.color(.waiting))
                Spacer(minLength: 4)
                if let showColumn {
                    Button("Show column", action: showColumn)
                        .buttonStyle(.link)
                        .font(.caption)
                }
            }
            Text(prompt.question)
                .font(.callout)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                ForEach(Array(prompt.choices.enumerated()), id: \.offset) { index, label in
                    Button {
                        pick(index)
                    } label: {
                        HStack(spacing: tokens.spacing.s) {
                            Text("\(index + 1)").monospacedDigit().foregroundStyle(.secondary)
                            Text(label).multilineTextAlignment(.leading)
                            Spacer(minLength: 0)
                            if sending == index { ProgressView().controlSize(.small) }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.bordered)
                    .disabled(sending != nil)
                }
            }
            if let failure {
                Text(failure).font(.caption).foregroundStyle(.red)
            }
        }
        .padding(tokens.spacing.m)
        .background(DesignTokens.promptFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.card))
        .overlay(RoundedRectangle(cornerRadius: DesignTokens.Radius.card).strokeBorder(DesignTokens.promptBorder))
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        // As in the CLI: while the card has focus, 1 to 9 pick a choice. ⌘1–⌘9 stay with the tabs.
        .onKeyPress(characters: .decimalDigits, phases: .down) { press in
            guard press.modifiers.isEmpty, let digit = Int(press.characters),
                  (1...prompt.choices.count).contains(digit) else { return .ignored }
            pick(digit - 1)
            return .handled
        }
        .onChange(of: prompt.id) {
            sending = nil
            failure = nil
        }
        .transition(reduceMotion ? .identity : .opacity)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(agent) is waiting: \(prompt.question)")
    }

    private func pick(_ index: Int) {
        guard sending == nil else { return }
        sending = index
        failure = nil
        Task {
            do {
                try await answer(index)
            } catch {
                failure = (error as? SwarmProfileError)?.message ?? String(describing: error)
                sending = nil
            }
        }
    }
}
