import SwiftUI
import SwarmCore

/// A provider as a round letter mark. The repo ships no provider logos.
struct ProviderMark: View {
    let provider: String

    var body: some View {
        Text(Self.letter(provider))
            .font(.caption2.weight(.bold))
            .frame(width: DesignTokens.Size.providerMark, height: DesignTokens.Size.providerMark)
            .overlay(Circle().strokeBorder(.secondary, lineWidth: DesignTokens.Size.hairline))
            .accessibilityHidden(true)
    }

    static func letter(_ provider: String) -> String {
        switch provider {
        // Codex takes X so it does not read as a second Claude.
        case "codex": "X"
        case "agy": "G"
        default: String(provider.prefix(1)).uppercased()
        }
    }
}

/// A profile's health as a symbol and a word, so color is never the only signal. Nil is a check
/// that has not come back or failed.
struct ProfileHealthPill: View {
    let status: ProfileStatus?

    var body: some View {
        Label(status?.title ?? "Status unknown", systemImage: symbol)
            .font(.caption)
            .foregroundStyle(color)
            .help(status?.text ?? "The launch check has not answered.")
    }

    private var symbol: String {
        switch status?.kind {
        case .primary: "circle.fill"
        case .fallback: "exclamationmark.triangle.fill"
        case .none?: "xmark.octagon.fill"
        case nil: "circle.dotted"
        }
    }

    private var color: Color {
        switch status?.kind {
        case .primary: DesignTokens.color(.done)
        case .fallback: DesignTokens.color(.waiting)
        case .none?: DesignTokens.color(.failed)
        case nil: .secondary
        }
    }
}

extension ProfileStatus {
    /// The fill behind a row or card: none while healthy or unknown.
    var fill: Color {
        switch kind {
        case .primary: .clear
        case .fallback: DesignTokens.warningFill
        case .none: DesignTokens.errorFill
        }
    }
}

/// A profile's runners in order as chips, marked from the last launch check. It shows as many
/// chips as the width allows, then "+N"; the chip the next launch takes always stays.
struct RunnerChain: View {
    let runners: [SwarmRunner]
    let check: SwarmProfileCheck?
    var providers: [SwarmProvider] = []
    /// A chip click, with the runner's index. Nil makes the chips plain text.
    var onSelect: ((Int) -> Void)?

    var body: some View {
        let states = RunnerChipState.states(runners: runners.count, check: check)
        ViewThatFits(in: .horizontal) {
            ForEach(Array(stride(from: runners.count, through: 1, by: -1)), id: \.self) { limit in
                chain(ChainFit(count: runners.count, pick: check?.pick, limit: limit), states: states)
            }
        }
    }

    private func chain(_ fit: ChainFit, states: [RunnerChipState]) -> some View {
        HStack(spacing: DesignTokens.Spacing.xs) {
            if fit.leadingCut {
                Text("…").foregroundStyle(.secondary)
                    .help(hiddenList(0..<(fit.shown.first ?? 0)))
            }
            ForEach(Array(fit.shown.enumerated()), id: \.element) { position, index in
                if position > 0 || fit.leadingCut {
                    Image(systemName: "arrow.right").font(.caption2).foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                }
                chip(index, state: states[index])
            }
            let rest = ((fit.shown.last ?? 0) + 1)..<runners.count
            if !rest.isEmpty {
                Text("+\(rest.count)").font(.caption).foregroundStyle(.secondary)
                    .help(hiddenList(rest))
                    .accessibilityLabel("\(rest.count) more: \(hiddenList(rest))")
            }
        }
        .fixedSize()
    }

    @ViewBuilder
    private func chip(_ index: Int, state: RunnerChipState) -> some View {
        let content = ChipContent(runner: runners[index], state: state)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(accessibilityText(index, state: state))
        if let onSelect {
            Button { onSelect(index) } label: { content }
                .buttonStyle(.plain)
                .accessibilityHint("Opens the editor at this runner")
        } else {
            content
        }
    }

    private func label(_ provider: String) -> String {
        providers.first { $0.id == provider }?.label ?? provider
    }

    private func hiddenList(_ indices: Range<Int>) -> String {
        indices.map { "\($0 + 1). \(label(runners[$0].provider)) \(runners[$0].model), \(runners[$0].effort)" }
            .joined(separator: "\n")
    }

    private func accessibilityText(_ index: Int, state: RunnerChipState) -> String {
        let runner = runners[index]
        var parts = ["Runner \(index + 1)", label(runner.provider), runner.model, runner.effort]
        switch state {
        case .normal: if check?.pick == index { parts.append("next launch") }
        case .next: parts.append("next launch")
        case .skipped(_, let full): parts.append("skipped, \(full)")
        }
        return parts.joined(separator: ", ")
    }
}

private struct ChipContent: View {
    let runner: SwarmRunner
    let state: RunnerChipState

    var body: some View {
        HStack(spacing: DesignTokens.Spacing.xs) {
            ProviderMark(provider: runner.provider)
            Text("\(runner.model)·\(Self.effort(runner.effort))")
                .font(DesignTokens.mono)
                .strikethrough(isSkipped)
            if case .skipped(let short, _) = state {
                Text("(\(short))").font(.caption)
            }
        }
        .foregroundStyle(isSkipped ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
        .padding(.horizontal, DesignTokens.Spacing.xs)
        .padding(.vertical, DesignTokens.Spacing.xxs)
        .background {
            if state == .next {
                RoundedRectangle(cornerRadius: DesignTokens.Radius.control)
                    .fill(DesignTokens.selectionAccentFill)
                    .strokeBorder(Color.accentColor, lineWidth: DesignTokens.Size.hairline)
            }
        }
        .help(help)
    }

    private var isSkipped: Bool {
        if case .skipped = state { return true }
        return false
    }

    private var help: String {
        switch state {
        case .normal: "\(runner.provider) \(runner.model), \(runner.effort) effort"
        case .next: "Next launch: \(runner.provider) \(runner.model), \(runner.effort) effort"
        case .skipped(_, let full): "Skipped: \(full)"
        }
    }

    static func effort(_ effort: String) -> String {
        effort == "medium" ? "med" : effort
    }
}
