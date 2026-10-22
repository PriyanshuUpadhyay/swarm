import SwarmHUDCore
import SwiftUI

// Look 2, "Table", as reviewed in tmp/flow/2026-10-15-notch-hud/design/r3/review.md: a pure black
// shape that grows out of the camera notch into one table with a row per account, no scroll and no
// fold. The time of day changes only the row stripe, the calm bar fill and a small glyph; warning
// colours never change.

extension Color {
    init(hex: UInt32) {
        self.init(red: Double(hex >> 16 & 0xFF) / 255, green: Double(hex >> 8 & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }
}

enum Palette {
    static let primary = Color(hex: 0xF2F2F7)
    static let secondary = Color(hex: 0xA8A8B0)
    static let tertiary = Color(hex: 0x8F8F94)
    static let warn = Color(hex: 0xFFA049)
    static let critical = Color(hex: 0xFF5A4F)
    /// The mascot's sweat drop: a cue, not a warning, so it is not one of the severity colours.
    static let sweat = Color(hex: 0x7FC4FF)
    static let track = Color.white.opacity(0.10)
    /// Brighter than `track`: a 3 pt ring needs more contrast than a 6 pt bar.
    static let ringTrack = Color.white.opacity(0.18)
    static let rule = Color.white.opacity(0.08)

    /// Text colour for a reading. Calm text stays white, so the phase never changes text contrast.
    static func ink(_ severity: Severity) -> Color {
        switch severity {
        case .calm: return primary
        case .warn: return warn
        case .critical: return critical
        }
    }
}

struct Theme {
    /// Every other row.
    let stripe: Color
    /// Calm bars and the status dot; never text.
    let calm: Color
    let symbol: String

    init(_ phase: DayPhase) {
        switch phase {
        case .dawn: (stripe, calm, symbol) = (Color(hex: 0x16161A), Color(hex: 0xB8B0C8), "sunrise")
        case .day: (stripe, calm, symbol) = (Color(hex: 0x1C1C1E), Color(hex: 0xD9D6CE), "sun.max")
        case .dusk: (stripe, calm, symbol) = (Color(hex: 0x16161A), Color(hex: 0xC2AFA6), "sunset")
        case .night: (stripe, calm, symbol) = (Color(hex: 0x0F1012), Color(hex: 0x8C9AC0), "moon")
        }
    }

    func fill(_ severity: Severity) -> Color { severity == .calm ? calm : Palette.ink(severity) }
}

enum Layout {
    /// Each closed wing beside the camera: the closed ear, then 40 pt for the 24 pt tall mascot or
    /// the 15 pt ring. Wide guests such as the crab shrink to the 40 pt.
    static let wing: CGFloat = 46
    static let closedMascot: CGFloat = 24
    /// The open card's mascot, in the header beside the answer.
    static let openMascot: CGFloat = 40
    /// The closed shape's concave top corners.
    static let closedEar: CGFloat = 6
    /// The widest open card; fewer limit kinds make it narrower.
    static let openWidth: CGFloat = 1088
    /// The narrowest: the header's two sides still fit beside the camera.
    static let minOpenWidth: CGFloat = 820
    /// 11 accounts fit in the compact rows; the panel adds room for the shadow.
    static let maxOpenHeight: CGFloat = 660
    /// The open shape's concave top corners, which the content must clear.
    static let ear: CGFloat = 14
    static let inset: CGFloat = ear + 12
    static let rowPadding: CGFloat = 8
    static let belowStatus: CGFloat = 6
    /// The header: the mascot and two lines of answer, never shorter than the notch.
    static func header(notch: CGSize) -> CGFloat { max(notch.height, 56) }
    /// The running-agents line, only while agents run.
    static let running: CGFloat = 32
    static let headings: CGFloat = 24
    /// Two lines (use and reset, then the pace) up to `denseAfter` rows, then one line.
    static func row(dense: Bool) -> CGFloat { dense ? 34 : 42 }
    static let denseAfter = 5
    /// The line for accounts with no reading, with its gap above.
    static let needs: CGFloat = 40
    static let bottom: CGFloat = 10
    static let markColumn: CGFloat = 20
    static let nameColumn: CGFloat = 150
    static let readingColumn: CGFloat = 104
    static let valueColumn: CGFloat = 40
    /// Holds a reset ("6d 23h") or, in a one-line row, a warning pace ("full 16h 30m").
    static let resetColumn: CGFloat = 84
    /// A limit column's width when the card is not at its widest.
    static let idealCell: CGFloat = 280
    static let gap: CGFloat = 12

    /// The mark, name and, when rows differ, Reading columns with the gap after each.
    static func lead(reading: Bool) -> CGFloat {
        markColumn + nameColumn + gap * 2 + (reading ? readingColumn + gap : 0)
    }

    /// Wide enough for `kinds` columns of `idealCell`, so a Codex-only card has no 800 pt bar.
    static func cardWidth(kinds: Int, reading: Bool) -> CGFloat {
        let table = lead(reading: reading) + CGFloat(kinds) * idealCell + CGFloat(max(kinds - 1, 0)) * gap
        return min(max(2 * inset + 2 * rowPadding + table, minOpenWidth), openWidth)
    }
    /// The least notch height the HUD assumes, also the closed height on a Mac with no notch reading.
    static let minNotch: CGFloat = 37
}

extension UsageModel {
    var closedSize: CGSize {
        CGSize(width: max(notch.width, 32) + 2 * Layout.wing, height: notch.height)
    }

    /// The reading of every account with windows when all of them read the same, such as
    /// "Stale · 48m", "" when all are fresh, or nil when rows differ and need the Reading column.
    func sharedReading(now: Date) -> String? {
        let readings = Set(accounts.filter { !$0.windows.isEmpty }
            .map { readingText($0, readFailed: readError != nil, readAt: readAt, now: now) ?? "" })
        return readings.count <= 1 ? readings.first ?? "" : nil
    }

    var openSize: CGSize {
        let table = ledger(accounts)
        var height = Layout.header(notch: notch) + Layout.belowStatus + Layout.bottom
        if !agents.agents.isEmpty { height += Layout.running }
        if !table.rows.isEmpty {
            height += Layout.headings + CGFloat(table.rows.count) * Layout.row(dense: table.rows.count > Layout.denseAfter)
        }
        if !table.signedOut.isEmpty { height += Layout.needs }
        let width = Layout.cardWidth(kinds: table.kinds.count, reading: sharedReading(now: now ?? Date()) == nil)
        return CGSize(width: width, height: min(height, Layout.maxOpenHeight))
    }
}

/// The notch outline: concave ears at the top corners, rounded bottom corners.
struct NotchShape: Shape {
    var ear: CGFloat
    var radius: CGFloat

    var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get { AnimatablePair(ear, radius) }
        set { (ear, radius) = (newValue.first, newValue.second) }
    }

    func path(in rect: CGRect) -> Path {
        let ear = min(ear, rect.height / 3), radius = min(radius, rect.height / 2)
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addQuadCurve(to: CGPoint(x: rect.minX + ear, y: rect.minY + ear), control: CGPoint(x: rect.minX + ear, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.minX + ear, y: rect.maxY - radius))
        path.addQuadCurve(to: CGPoint(x: rect.minX + ear + radius, y: rect.maxY), control: CGPoint(x: rect.minX + ear, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.maxX - ear - radius, y: rect.maxY))
        path.addQuadCurve(to: CGPoint(x: rect.maxX - ear, y: rect.maxY - radius), control: CGPoint(x: rect.maxX - ear, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.maxX - ear, y: rect.minY + ear))
        path.addQuadCurve(to: CGPoint(x: rect.maxX, y: rect.minY), control: CGPoint(x: rect.maxX - ear, y: rect.minY))
        path.closeSubpath()
        return path
    }
}

struct HUDView: View {
    let model: UsageModel
    /// False in `--snapshot`, where the clock is fixed and nothing animates.
    let live: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var open: Bool { model.isOpen }
    private var size: CGSize { open ? model.openSize : model.closedSize }
    private var shape: NotchShape { NotchShape(ear: open ? Layout.ear : Layout.closedEar, radius: open ? 28 : 13) }
    private var theme: Theme { Theme(model.phase) }

    private var geometryAnimation: Animation? {
        guard live, !reduceMotion else { return nil }
        // Opening may overshoot a little; closing must look like the panel simply left.
        return open ? .spring(response: 0.42, dampingFraction: 0.82) : .spring(response: 0.45, dampingFraction: 1)
    }

    var body: some View {
        TimelineView(.periodic(from: .now, by: live ? 30 : 3600)) { context in
            let now = model.now ?? context.date
            ZStack(alignment: .top) {
                Color.black
                if open {
                    OpenContent(model: model, theme: theme, live: live, now: now)
                        .transition(reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 0.97, anchor: .top)))
                } else {
                    ClosedWings(model: model, theme: theme, live: live, now: now)
                }
            }
            .frame(width: size.width, height: size.height, alignment: .top)
            .clipShape(shape)
            .shadow(color: .black.opacity(open ? 0.46 : 0), radius: open ? 13 : 0, y: open ? 10 : 0)
            .animation(geometryAnimation, value: open)
            .animation(live && !reduceMotion ? .easeInOut(duration: 2) : nil, value: model.phase)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .environment(\.colorScheme, .dark)
    }
}

func providerTitle(_ provider: String) -> String {
    switch provider {
    case "claude": return "Claude"
    case "codex": return "Codex"
    case "agy": return "AGY"
    default: return provider
    }
}

/// Gray, so only warnings carry colour; the two shapes still tell the providers apart.
struct ProviderMark: View {
    let provider: String
    var size: CGFloat = 14

    var body: some View {
        Canvas { context, canvas in
            let s = canvas.width / 14
            if provider == "claude" {
                for angle in stride(from: 0.0, to: 180, by: 45) {
                    var line = Path()
                    line.move(to: CGPoint(x: 7 * s, y: 1.5 * s))
                    line.addLine(to: CGPoint(x: 7 * s, y: 12.5 * s))
                    let turn = CGAffineTransform(translationX: 7 * s, y: 7 * s).rotated(by: angle * .pi / 180).translatedBy(x: -7 * s, y: -7 * s)
                    context.stroke(line.applying(turn), with: .color(Palette.secondary), style: .init(lineWidth: 1.7 * s, lineCap: .round))
                }
            } else {
                context.stroke(Path(roundedRect: CGRect(x: s, y: s, width: 12 * s, height: 12 * s), cornerRadius: 3.5 * s),
                               with: .color(Palette.secondary), lineWidth: 1.6 * s)
                var prompt = Path()
                prompt.move(to: CGPoint(x: 4.3 * s, y: 5.2 * s))
                prompt.addLine(to: CGPoint(x: 6.3 * s, y: 7 * s))
                prompt.addLine(to: CGPoint(x: 4.3 * s, y: 8.8 * s))
                prompt.move(to: CGPoint(x: 7.6 * s, y: 9 * s))
                prompt.addLine(to: CGPoint(x: 10 * s, y: 9 * s))
                context.stroke(prompt, with: .color(Palette.secondary), style: .init(lineWidth: 1.4 * s, lineCap: .round))
            }
        }
        .frame(width: size, height: size)
        .accessibilityLabel(providerTitle(provider))
    }
}

/// "2h 10m" until 100%; under a minute still reads "1m", because shortDuration says "now" there.
func fillDuration(_ seconds: TimeInterval, now: Date) -> String {
    shortDuration(until: now.addingTimeInterval(max(seconds, 60)), now: now)
}

extension BestAccount {
    /// "98% left", "18% left · full in 3h 10m", "all full · resets in 2h": the room left on the
    /// fullest limit, then a time only when one fills before its reset, so a HUD with no pace yet
    /// never claims that the account lasts.
    func paceText(now: Date) -> String {
        let left = "\(100 - fullest)% left"
        switch outlook {
        case .left(let seconds): return "\(left) · full in \(fillDuration(seconds, now: now))"
        case .lasts: return left
        case .allFull(let resetAt): return "all full" + (resetAt.map { " · resets in \(shortDuration(until: $0, now: now))" } ?? "")
        }
    }

    func spoken(now: Date) -> String {
        switch outlook {
        case .left(let seconds):
            return "Best account \(account.name), \(fullest) percent used, a limit fills in \(fillDuration(seconds, now: now))"
        case .lasts: return "Best account \(account.name), \(fullest) percent used"
        case .allFull(let resetAt):
            return "All accounts full" + (resetAt.map { ", \(account.name) resets in \(shortDuration(until: $0, now: now))" } ?? "")
        }
    }
}

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
/// limit as a ring on the right, nothing over the camera and no text.
struct ClosedWings: View {
    let model: UsageModel
    let theme: Theme
    let live: Bool
    let now: Date

    var body: some View {
        let best = bestAccount(model.accounts, history: model.history, now: now)
        HStack(spacing: 0) {
            model.mascot(now: now, height: Layout.closedMascot, live: live)
                .frame(width: Layout.wing - Layout.closedEar)
                .padding(.leading, Layout.closedEar)
            Spacer(minLength: 0)
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
        .accessibilityLabel(agentsSpoken(model.agents) + ", " + (best.map { lowercasedFirst($0.spoken(now: now)) } ?? "no usage reading")
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
