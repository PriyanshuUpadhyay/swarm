// The closed wings: the mascot, the running count and models, and the usage ring, with their spoken text.
import SwarmHUDCore
import SwiftUI

/// "2 Claude agents working, Codex agent waiting for you", "No agent working".
func agentsSpoken(_ agents: WorkingSnapshot) -> String {
    func phrase(_ counts: [String: Int], _ verb: String) -> [String] {
        mascotFamilies.compactMap { provider in
            counts[provider].map { $0 == 1 ? "\(providerTitle(provider)) agent \(verb)" : "\($0) \(providerTitle(provider)) agents \(verb)" }
        }
    }
    let parts = phrase(agents.working, "working") + phrase(agents.waiting, "waiting for you")
    return parts.isEmpty ? "No agent working" : parts.joined(separator: ", ")
}

/// "Claude on opus, Codex on gpt-6": each model beside its provider, so none reads as another's.
func modelsSpoken(_ agents: [RunningAgent]) -> String {
    var phrases: [String] = []
    for agent in agents where !agent.waiting {
        let phrase = "\(providerTitle(agent.provider)) on \(modelLabel(agent.model, provider: agent.provider))"
        if !phrases.contains(phrase) { phrases.append(phrase) }
    }
    return phrases.joined(separator: ", ")
}

extension UsageModel {
    func mascotTurn(now: Date) -> Int { fixedMascot ?? SwarmHUDCore.mascotTurn(at: now, count: MascotCast.count) }

    /// The mascot that shows now, doing what the agents do; the closed wing and the open card both show it.
    func mascot(now: Date, height: CGFloat, live: Bool) -> Mascot {
        let best = bestAccount(accounts, history: history, now: now)
        return Mascot(family: mascotFamily(agents, lastWorking: lastWorking, best: best),
                      state: mascotState(agents, lastActive: lastActive, now: now),
                      turn: mascotTurn(now: now), height: height, live: live, strained: best?.strained ?? false,
                      crowd: agents.working.values.reduce(0, +))
    }
}

/// Closed: a pixel mascot that shows what the agents do on the left, the best account's fullest
/// limit as a ring on the right, and nothing over the camera. While agents run, the wings grow to
/// say how many and which models, or that one needs the owner; at 95% the ring says its number.
struct ClosedWings: View {
    let model: UsageModel
    let theme: Theme
    let live: Bool
    let now: Date

    var body: some View {
        let best = bestAccount(model.accounts, history: model.history, now: now)
        let text = model.wings
        HStack(spacing: Layout.wingGap) {
            model.mascot(now: now, height: Layout.closedMascot, live: live)
                .frame(width: Layout.wing - Layout.closedEar)
                .padding(.leading, Layout.closedEar)
            if let left = text.left {
                Text(left).font(.system(size: Layout.wingFont, weight: .semibold).monospacedDigit())
                    .foregroundStyle(text.waiting ? Palette.warn : Palette.secondary)
                    .fixedSize()
            }
            Spacer(minLength: 0)
            ForEach(text.tags, id: \.self) { tag in
                Text(tag).font(.system(size: Layout.tagFont, weight: .semibold)).foregroundStyle(text.waiting ? Palette.warn : Palette.secondary)
                    .fixedSize()
                    .padding(.horizontal, Layout.tagPadding).padding(.vertical, 2)
                    .background(Capsule().fill(Color.white.opacity(0.12)))
            }
            if let number = text.number {
                Text(number).font(.system(size: Layout.wingFont, weight: .semibold).monospacedDigit()).foregroundStyle(Palette.critical).fixedSize()
            }
            UsageRingView(ring: usageRing(best, readFailed: model.readError != nil), theme: theme)
                .overlay(alignment: .topTrailing) {
                    // A reset would help now; the open card says how many and until when.
                    if best?.offersReset == true {
                        Circle().fill(Palette.warn).frame(width: 5, height: 5).offset(x: 3, y: -3)
                    }
                }
                .frame(width: Layout.wing - Layout.closedEar)
                .padding(.trailing, Layout.closedEar)
        }
        .frame(maxHeight: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(agentsSpoken(model.agents) + (text.tags.isEmpty || text.waiting ? "" : ", " + modelsSpoken(model.agents.agents))
                            + ", " + (best.map { lowercasedFirst($0.spoken(now: now)) } ?? "no usage reading")
                            + (best?.offersReset == true ? ", a Codex reset is available" : ""))
        .accessibilityAddTraits(.isButton)
        .accessibilityHint(model.hotKeyReady ? "Opens the usage card. Control Option Shift Command U also opens it." : "Opens the usage card.")
        .accessibilityAction { model.toggleFromKeyboard?() }
    }

    private func lowercasedFirst(_ text: String) -> String { text.prefix(1).lowercased() + text.dropFirst() }
}

/// 15 pt across with a 3 pt line. Old numbers draw at half strength; no reading draws a dim empty ring.
struct UsageRingView: View {
    let ring: UsageRing
    let theme: Theme

    var body: some View {
        ZStack {
            Circle().stroke(Palette.ringTrack, lineWidth: 3)
            if case .reading(let used, let severity, _) = ring, used > 0 {
                Circle().trim(from: 0, to: Double(min(used, 100)) / 100)
                    .stroke(theme.fill(severity), style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
        }
        // The stroke straddles the path, so a 12 pt circle has a 15 pt outer edge.
        .frame(width: 12, height: 12)
        .padding(1.5)
        .opacity(opacity)
    }

    private var opacity: Double {
        switch ring {
        case .none: return 0.6
        case .reading(_, _, let old): return old ? 0.5 : 1
        }
    }
}
