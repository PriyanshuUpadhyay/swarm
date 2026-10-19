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
    static let openMascot: CGFloat = 72
    /// The closed shape's concave top corners.
    static let closedEar: CGFloat = 6
    static let openWidth: CGFloat = 1088
    /// 11 accounts in two provider groups fit; the panel adds room for the shadow.
    static let maxOpenHeight: CGFloat = 660
    /// The open shape's concave top corners, which the content must clear.
    static let ear: CGFloat = 14
    static let inset: CGFloat = ear + 12
    static let rowPadding: CGFloat = 8
    static let belowStatus: CGFloat = 12
    static let headings: CGFloat = 24
    /// Two lines: use and reset, then the pace.
    static let row: CGFloat = 42
    /// The legend line, with the open mascot and its credit on the right.
    static let legend: CGFloat = Layout.openMascot + 12
    /// The least height of the last group that holds the mascot, its credit and the legend beside it.
    static let besideHeight: CGFloat = 150
    static let bottom: CGFloat = 10
    static let markColumn: CGFloat = 20
    static let nameColumn: CGFloat = 150
    static let readingColumn: CGFloat = 104
    static let valueColumn: CGFloat = 40
    static let resetColumn: CGFloat = 64
    static let gap: CGFloat = 12

    /// The mark, name and, when rows differ, Reading columns with the gap after each.
    static func lead(reading: Bool) -> CGFloat {
        markColumn + nameColumn + gap * 2 + (reading ? readingColumn + gap : 0)
    }

    /// One window column's width when `columns` share the row, so a narrower group lines up.
    static func cell(columns: Int, reading: Bool) -> CGFloat {
        let fixed = lead(reading: reading) + CGFloat(columns - 1) * gap
        return (openWidth - 2 * inset - 2 * rowPadding - fixed) / CGFloat(max(columns, 1))
    }

    /// The width of a group with `kinds` columns, its padding included, in a table of `columns`.
    static func groupWidth(kinds: Int, columns: Int, reading: Bool) -> CGFloat {
        2 * rowPadding + lead(reading: reading) + CGFloat(kinds) * cell(columns: columns, reading: reading)
            + CGFloat(kinds - 1) * gap
    }
    /// The least notch height the HUD assumes, also the closed height on a Mac with no notch reading.
    static let minNotch: CGFloat = 37

    /// The status line sits in the band beside the camera, as tall as the notch.
    static func band(notch: CGSize) -> CGFloat { max(notch.height, minNotch) }
}

extension UsageModel {
    var closedSize: CGSize {
        CGSize(width: max(notch.width, 32) + 2 * Layout.wing, height: notch.height)
    }

    /// The legend, credit and mascot fill the empty columns beside the last group when it has
    /// fewer columns and is tall enough, so the card needs no band of its own for them.
    var footerBeside: Bool {
        let groups = providerGroups(accounts)
        guard let last = groups.last, let widest = groups.map(\.kinds.count).max() else { return false }
        return last.kinds.count < widest
            && Layout.headings + CGFloat(last.accounts.count) * Layout.row >= Layout.besideHeight
    }

    /// The reading of every account with windows when all of them read the same, such as
    /// "Stale · 48m", "" when all are fresh, or nil when rows differ and need the Reading column.
    func sharedReading(now: Date) -> String? {
        let readings = Set(accounts.filter { !$0.windows.isEmpty }
            .map { readingText($0, readFailed: readError != nil, readAt: readAt, now: now) ?? "" })
        return readings.count <= 1 ? readings.first ?? "" : nil
    }

    var openSize: CGSize {
        let height = Layout.band(notch: notch) + Layout.belowStatus
            + CGFloat(providerGroups(accounts).count) * Layout.headings
            + CGFloat(accounts.count) * Layout.row + (footerBeside ? 0 : Layout.legend) + Layout.bottom
        return CGSize(width: Layout.openWidth, height: min(height, Layout.maxOpenHeight))
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
    /// "2% used", "82% used · full in 3h 10m", "all full · resets in 2h": the used percent of the
    /// fullest limit, then a time only when one fills before its reset, so a HUD with no pace yet
    /// never claims that the account lasts.
    func paceText(now: Date) -> String {
        let used = "\(fullest)% used"
        switch outlook {
        case .left(let seconds): return "\(used) · full in \(fillDuration(seconds, now: now))"
        case .lasts: return used
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

struct OpenContent: View {
    let model: UsageModel
    let theme: Theme
    let live: Bool
    let now: Date

    var body: some View {
        let groups = providerGroups(model.accounts)
        // Every group draws as many columns as the widest one, so a column is as wide in each group.
        let columns = groups.map(\.kinds.count).max() ?? 0
        VStack(spacing: 0) {
            status.frame(height: Layout.band(notch: model.notch))
            Spacer().frame(height: Layout.belowStatus)
            ForEach(groups) { group in
                if model.footerBeside && group.id == groups.last?.id {
                    HStack(alignment: .top, spacing: 0) {
                        table(group, columns: group.kinds.count)
                            .frame(width: Layout.groupWidth(kinds: group.kinds.count, columns: columns,
                                                            reading: model.sharedReading(now: now) == nil))
                        VStack(alignment: .trailing, spacing: 6) {
                            Spacer(minLength: 0)
                            HStack(alignment: .bottom, spacing: 12) { credit; mascot }
                            legend.multilineTextAlignment(.trailing)
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(Palette.tertiary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .trailing)
                        .padding(.leading, Layout.gap)
                        .padding(.horizontal, Layout.rowPadding)
                        .padding(.bottom, 6)
                    }
                    .fixedSize(horizontal: false, vertical: true)
                } else {
                    table(group, columns: columns)
                }
            }
            if !model.footerBeside {
                HStack(spacing: 12) {
                    legend.frame(maxWidth: .infinity, alignment: .leading)
                    credit
                    mascot
                }
                .font(.system(size: 11))
                .foregroundStyle(Palette.tertiary)
                .frame(height: Layout.legend)
                .padding(.horizontal, Layout.rowPadding)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Layout.inset)
    }

    /// One provider's heading and rows, drawn `columns` wide so each column lines up across groups.
    private func table(_ group: ProviderGroup, columns: Int) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: Layout.gap) {
                Spacer().frame(width: Layout.markColumn)
                Text(providerTitle(group.provider)).frame(width: Layout.nameColumn, alignment: .leading)
                if model.sharedReading(now: now) == nil {
                    Text("Reading").frame(width: Layout.readingColumn, alignment: .leading)
                }
                ForEach(group.kinds, id: \.self) { kind in
                    Text(kind.title).frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(group.kinds.count..<columns, id: \.self) { _ in Color.clear.frame(maxWidth: .infinity) }
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(Palette.tertiary)
            .padding(.horizontal, Layout.rowPadding)
            .frame(height: Layout.headings)
            // Each row's label names its windows, so VoiceOver reads only the group name here.
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(providerTitle(group.provider))
            .accessibilityAddTraits(.isHeader)
            // Account order is the reader's, so a sign-in never moves a row.
            ForEach(Array(group.accounts.enumerated()), id: \.element.id) { index, account in
                Group {
                    if account.windows.isEmpty {
                        SignedOutRow(account: account, renewing: model.renewing.contains(account.id), onRenew: model.renew)
                    } else {
                        AccountRow(account: account, kinds: group.kinds, columns: columns, theme: theme, now: now,
                                   showsReading: model.sharedReading(now: now) == nil,
                                   history: model.history, readFailed: model.readError != nil, readAt: model.readAt)
                    }
                }
                .padding(.horizontal, Layout.rowPadding)
                .background(index.isMultiple(of: 2) ? Color.clear : theme.stripe)
                .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
            }
        }
    }

    private var legend: some View {
        Text("Each limit shows used % · resets in; the line below says when it fills at this pace. A dash means no reading.")
    }

    private var mascot: some View {
        model.mascot(now: now, height: Layout.openMascot, live: live)
            .frame(width: 2 * Layout.openMascot, height: Layout.openMascot)
    }

    /// Who the mascot is and who drew it; the credit is also what the guests' licences ask for.
    private var credit: some View {
        let turn = model.mascotTurn(now: now)
        return VStack(alignment: .trailing, spacing: 2) {
            Text(MascotCast.name(turn)).foregroundStyle(Palette.secondary)
            Text(MascotCast.guest(turn)?.credit ?? "swarm's own art")
        }
        .multilineTextAlignment(.trailing)
        .frame(maxWidth: 260, alignment: .trailing)
    }

    /// The best account and its pace left of the camera, the phase and read age right of it.
    private var status: some View {
        let wing = max((Layout.openWidth - 2 * Layout.inset - model.notch.width) / 2 - 12, 0)
        return HStack(spacing: 0) {
            HStack(spacing: 8) {
                if let best = bestAccount(model.accounts, history: model.history, now: now) {
                    Circle().fill(theme.fill(best.severity)).frame(width: 7, height: 7)
                    // A long name gives way; the pace always shows.
                    HStack(spacing: 0) {
                        Text("Best ").foregroundStyle(Palette.tertiary).fixedSize()
                        if best.isOld(readFailed: model.readError != nil) { Text("~").fixedSize() }
                        Text(best.account.name).lineLimit(1).truncationMode(.middle)
                        Text(" · \(providerTitle(best.account.provider)) · ").fixedSize()
                        Text(best.paceText(now: now)).fixedSize()
                            .foregroundStyle(best.severity == .calm ? Palette.primary : Palette.ink(best.severity))
                    }
                    .foregroundStyle(Palette.primary)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(best.spoken(now: now))
                } else {
                    // The cause names the fix (for example "swarm not found; set SWARM_BIN").
                    Text(model.readError.map { "\($0) · retries each minute" } ?? "No usage reading")
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(1).truncationMode(.tail)
                        .help(model.readError ?? "")
                }
            }
            .frame(width: wing, alignment: .leading)
            Spacer(minLength: 0)
            HStack(spacing: 6) {
                Image(systemName: theme.symbol).font(.system(size: 11)).accessibilityHidden(true)
                Text(model.phase.title)
                Text("·").accessibilityHidden(true)
                if let refreshError = model.refreshError {
                    Text(refreshError).foregroundStyle(Palette.warn).lineLimit(1).truncationMode(.tail)
                } else {
                    Text(age).monospacedDigit()
                }
                // Every row read the same, so the table drops its Reading column and says it here.
                if let shared = model.sharedReading(now: now), !shared.isEmpty {
                    Text("·").accessibilityHidden(true)
                    Text("all \(shared.prefix(1).lowercased() + shared.dropFirst())").foregroundStyle(Palette.warn).monospacedDigit()
                }
                Button { model.refresh() } label: {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 11, weight: .semibold))
                        .rotationEffect(.degrees(model.refreshing.isEmpty ? 0 : 180))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(model.canRefresh ? Palette.secondary : Palette.tertiary.opacity(0.6))
                .disabled(!model.canRefresh)
                .help(model.refreshError ?? (model.canRefresh ? "Refresh usage" : "Next refresh in a few minutes"))
                .accessibilityLabel("Refresh usage")
            }
            .foregroundStyle(Palette.tertiary)
            .frame(width: wing, alignment: .trailing)
        }
        .font(.system(size: 12, weight: .medium))
    }

    private var age: String {
        if model.readError != nil { return "read failed" }
        guard let readAt = model.readAt else { return "reading…" }
        let minutes = Int(now.timeIntervalSince(readAt) / 60)
        return minutes < 1 ? "updated now" : "updated \(minutes)m ago"
    }
}

/// The account name.
struct AccountName: View {
    let account: UsageAccount
    let color: Color

    var body: some View {
        // The group heading and the mark name the provider, so the name stands alone.
        VStack(alignment: .leading, spacing: 2) {
            Text(account.name).font(.system(size: 13, weight: .medium)).foregroundStyle(color)
                .lineLimit(1).truncationMode(.middle).help(account.name)
            // A Codex account's resets sit under its name, where the row has a free second line.
            if let credits = account.resetCredits, let short = credits.short() {
                Text(short).font(.system(size: 11)).foregroundStyle(Palette.tertiary).lineLimit(1)
                    .help("\(credits.text() ?? short)\nRedeem one in ChatGPT settings.")
                    .accessibilityLabel(credits.text() ?? short)
            }
        }
        .frame(width: Layout.nameColumn, alignment: .leading)
    }
}

struct AccountRow: View {
    let account: UsageAccount
    let kinds: [WindowKind]
    /// The widest group's column count; the rest of the row stays empty.
    let columns: Int
    let theme: Theme
    let now: Date
    /// False when every row reads the same; the status line then says it once.
    let showsReading: Bool
    let history: [HistorySample]
    /// The last read failed, so every number on screen is at least as old as `readAt`.
    let readFailed: Bool
    let readAt: Date?

    var body: some View {
        HStack(spacing: Layout.gap) {
            // The mark's label is the only place the row names its provider.
            ProviderMark(provider: account.provider, size: 16).frame(width: Layout.markColumn)
            AccountName(account: account, color: Palette.primary)
            if showsReading {
                reading.font(.system(size: 11)).lineLimit(1).frame(width: Layout.readingColumn, alignment: .leading)
            }
            ForEach(kinds, id: \.self) { kind in
                if let window = account.windows.first(where: { $0.kind == kind }) {
                    WindowCell(window: window, pace: pace(account: account, window: window, history: history, now: now),
                               theme: theme, now: now)
                } else {
                    Text("—").font(.system(size: 12)).foregroundStyle(Palette.tertiary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityLabel("\(kind.title) no reading")
                }
            }
            ForEach(kinds.count..<columns, id: \.self) { _ in Color.clear.frame(maxWidth: .infinity) }
        }
        .accessibilityElement(children: .combine)
        .frame(height: Layout.row)
    }

    @ViewBuilder private var reading: some View {
        if let stale = readingText(account, readFailed: readFailed, readAt: readAt, now: now) {
            Text(stale).foregroundStyle(Palette.warn)
        } else {
            Text("fresh").foregroundStyle(Palette.tertiary)
        }
    }
}

/// "Stale · 45m" for an account with an old window, nil when every window is fresh. Like the closed
/// wings' `~`, a failed read marks even fresh windows as old.
func readingText(_ account: UsageAccount, readFailed: Bool, readAt: Date?, now: Date) -> String? {
    guard readFailed || account.windows.contains(where: { !$0.isFresh }) else { return nil }
    let ages = account.windows.filter { readFailed || !$0.isFresh }.compactMap(\.asOf) + (readFailed ? [readAt].compactMap { $0 } : [])
    guard let oldest = ages.min() else { return "Stale" }
    // shortDuration says "now" under a minute, which reads wrong for an age.
    let age = now.timeIntervalSince(oldest) < 60 ? "<1m" : shortDuration(until: now, now: oldest)
    return "Stale · \(age)"
}

struct WindowCell: View {
    let window: UsageWindow
    /// nil while there are too few samples; the second line then stays blank.
    let pace: Pace?
    let theme: Theme
    let now: Date

    private var paceText: String? {
        switch pace {
        case .full: return "full now"
        case .fillsIn(let seconds): return "full in \(fillDuration(seconds, now: now))"
        case .lasts: return "lasts to reset"
        case nil: return nil
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            usage
            // A blank line keeps the first line where it is in every row.
            Text(paceText ?? " ")
                .font(.system(size: 11).monospacedDigit())
                .foregroundStyle(pace.map { $0 == .lasts ? Palette.tertiary : Palette.ink($0.severity(window: window)) } ?? Palette.tertiary)
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(window.kind.title) \(window.usedPct) percent used"
            + (window.severity == .critical ? ", critical" : "")
            + (window.resetAt.map { ", resets in \(shortDuration(until: $0, now: now))" } ?? "")
            + (paceText.map { ", \($0)" } ?? ""))
    }

    private var usage: some View {
        HStack(spacing: 8) {
            UsageBar(fraction: Double(window.usedPct) / 100, color: theme.fill(window.severity))
            Text("\(window.usedPct)%")
                .font(.system(size: 12, weight: .semibold).monospacedDigit())
                .foregroundStyle(Palette.ink(window.severity))
                .overlay(alignment: .topTrailing) {
                    if window.severity == .critical {
                        Text("!").font(.system(size: 8, weight: .heavy)).foregroundStyle(Palette.critical).offset(x: 6, y: -3)
                    }
                }
                .frame(width: Layout.valueColumn, alignment: .trailing)
            Text(window.resetAt.map { shortDuration(until: $0, now: now) } ?? "—")
                .font(.system(size: 12).monospacedDigit())
                .foregroundStyle(Palette.tertiary)
                .lineLimit(1)
                .frame(width: Layout.resetColumn, alignment: .trailing)
        }
    }
}

/// A 6 pt bar with a tick at 80%, where the warning colour starts. Zero draws no fill.
struct UsageBar: View {
    let fraction: Double
    let color: Color

    var body: some View {
        GeometryReader { bar in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.track)
                if fraction > 0 {
                    Capsule().fill(color).frame(width: max(bar.size.width * min(fraction, 1), 6))
                }
                // Black on the fill, light gray on the bare track, so the mark shows either way.
                Rectangle().fill(fraction >= 0.8 ? Color.black.opacity(0.9) : Color.white.opacity(0.45))
                    .frame(width: 1).offset(x: bar.size.width * 0.8)
            }
        }
        .frame(height: 6)
    }
}

struct SignedOutRow: View {
    let account: UsageAccount
    let renewing: Bool
    let onRenew: (UsageAccount) -> Void

    var body: some View {
        HStack(spacing: Layout.gap) {
            ProviderMark(provider: account.provider, size: 16).frame(width: Layout.markColumn)
            AccountName(account: account, color: Palette.secondary)
            HStack(spacing: 6) {
                Text(account.problem ?? "No reading").font(.system(size: 12)).foregroundStyle(Palette.secondary)
                    .lineLimit(2).truncationMode(.tail).help(account.problem ?? "")
                if account.renewalArguments != nil {
                    if renewing {
                        ProgressView().controlSize(.mini)
                            .accessibilityLabel("Renewing this account's token")
                    } else {
                        Button("Renew") { onRenew(account) }
                            .buttonStyle(.plain)
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(Palette.warn)
                            .underline()
                            .help("Run Claude on \(account.name) in the background to renew its token, then refresh")
                            .accessibilityLabel("Renew this account's token")
                            .fixedSize()
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .frame(height: Layout.row)
    }
}
