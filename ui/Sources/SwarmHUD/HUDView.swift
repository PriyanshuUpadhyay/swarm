import SwarmHUDCore
import SwiftUI

// Look A, "Island", as reviewed in tmp/flow/2026-10-15-notch-hud/02-design.md: a pure black shape
// that grows out of the camera notch. The time of day tints only the lower field and the wing wash;
// warning colours never change.

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
    static let track = Color.white.opacity(0.10)
    static let rule = Color.white.opacity(0.08)
    static let claude = Color(hex: 0xD97757)
    static let codex = Color(hex: 0x7A9DFF)
}

struct Theme {
    let field: Color
    let bloom: Color
    let accent: Color

    init(_ phase: DayPhase) {
        switch phase {
        case .dawn: (field, bloom, accent) = (Color(hex: 0x241821), Color(hex: 0xE9B9D4), Color(hex: 0xE8C7F1))
        case .day: (field, bloom, accent) = (Color(hex: 0x0C1D26), Color(hex: 0x48B8E8), Color(hex: 0x5CD5FF))
        case .dusk: (field, bloom, accent) = (Color(hex: 0x20152B), Color(hex: 0xAA75D9), Color(hex: 0xC6A6FF))
        case .night: (field, bloom, accent) = (Color(hex: 0x101428), Color(hex: 0x6674C8), Color(hex: 0x8FA3FF))
        }
    }

    func color(_ severity: Severity) -> Color {
        switch severity {
        case .calm: return accent
        case .warn: return Palette.warn
        case .critical: return Palette.critical
        }
    }
}

enum Layout {
    /// Room for `~100%` at 12 pt (43.4 pt) plus 10 pt outer padding.
    static let wing: CGFloat = 56
    static let openWidth: CGFloat = 600
    static let maxOpenHeight: CGFloat = 460
    /// Black below the camera before any content or tint starts.
    static let cap: CGFloat = 53
    static let status: CGFloat = 36
    static let headings: CGFloat = 18
    static let row: CGFloat = 56
    static let fold: CGFloat = 32
    static let bottom: CGFloat = 14
    static let markColumn: CGFloat = 20
    static let nameColumn: CGFloat = 256
    static let windowColumn: CGFloat = 84
    static let gap: CGFloat = 8
    static let signedOutRow: CGFloat = 40
    /// The least notch height the HUD assumes, also the closed height on a Mac with no notch reading.
    static let minNotch: CGFloat = 37

    /// Where the open content starts: below the camera, with room before the status line.
    static func contentTop(notch: CGSize) -> CGFloat { max(notch.height, minNotch) + 16 }
}

extension UsageModel {
    var liveAccounts: [UsageAccount] { accounts.filter { !$0.windows.isEmpty } }
    var signedOut: [UsageAccount] { accounts.filter { $0.windows.isEmpty } }

    var closedSize: CGSize {
        CGSize(width: max(notch.width, 32) + 2 * Layout.wing, height: notch.height)
    }

    var openSize: CGSize {
        let rows = CGFloat(liveAccounts.count) * Layout.row + (foldOpen ? CGFloat(signedOut.count) * Layout.signedOutRow : 0)
        let fold = signedOut.isEmpty ? 0 : Layout.fold
        let height = Layout.contentTop(notch: notch) + Layout.status + Layout.headings + rows + fold + Layout.bottom
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
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast

    private var open: Bool { model.isOpen }
    private var size: CGSize { open ? model.openSize : model.closedSize }
    private var shape: NotchShape { NotchShape(ear: open ? 14 : 6, radius: open ? 28 : 13) }
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
                    field
                    OpenContent(model: model, theme: theme, now: now, live: live)
                        .padding(.top, Layout.contentTop(notch: model.notch))
                        .transition(reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 0.97, anchor: .top)))
                } else {
                    ClosedWings(model: model, theme: theme)
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

    /// The phase tint: black under the camera, then the field, with a soft bloom at the bottom.
    private var field: some View {
        ZStack(alignment: .bottom) {
            LinearGradient(stops: [
                .init(color: .clear, location: 0),
                .init(color: .clear, location: Layout.cap / size.height),
                .init(color: theme.field, location: min(1, (Layout.cap + 80) / size.height)),
            ], startPoint: .top, endPoint: .bottom)
            // A blur behind text is what Reduce Transparency and Increase Contrast ask to drop.
            if !reduceTransparency && contrast != .increased {
                Ellipse()
                    .fill(theme.bloom)
                    .frame(width: 160, height: 72)
                    .blur(radius: 24)
                    .opacity(0.28)
                    .offset(y: 36)
            }
        }
    }
}

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
                    context.stroke(line.applying(turn), with: .color(Palette.claude), style: .init(lineWidth: 1.7 * s, lineCap: .round))
                }
            } else {
                context.stroke(Path(roundedRect: CGRect(x: s, y: s, width: 12 * s, height: 12 * s), cornerRadius: 3.5 * s),
                               with: .color(Palette.codex), lineWidth: 1.6 * s)
                var prompt = Path()
                prompt.move(to: CGPoint(x: 4.3 * s, y: 5.2 * s))
                prompt.addLine(to: CGPoint(x: 6.3 * s, y: 7 * s))
                prompt.addLine(to: CGPoint(x: 4.3 * s, y: 8.8 * s))
                prompt.move(to: CGPoint(x: 7.6 * s, y: 9 * s))
                prompt.addLine(to: CGPoint(x: 10 * s, y: 9 * s))
                context.stroke(prompt, with: .color(Palette.codex), style: .init(lineWidth: 1.4 * s, lineCap: .round))
            }
        }
        .frame(width: size, height: size)
        .accessibilityLabel(provider == "claude" ? "Claude" : "Codex")
    }
}

/// Closed: the worst window's provider on the left and its use on the right, nothing over the camera.
struct ClosedWings: View {
    let model: UsageModel
    let theme: Theme

    var body: some View {
        let worst = worstWindow(model.accounts)
        HStack(spacing: 0) {
            ZStack(alignment: .topTrailing) {
                if let worst {
                    ProviderMark(provider: worst.account.provider)
                    if worst.window.severity == .critical {
                        Text("!").font(.system(size: 8, weight: .heavy)).foregroundStyle(Palette.critical).offset(x: 6, y: -4)
                    }
                } else {
                    Text("!").font(.system(size: 12, weight: .bold)).foregroundStyle(Palette.secondary)
                }
            }
            .frame(width: Layout.wing - 10, alignment: .leading)
            .padding(.leading, 10)
            Spacer(minLength: 0)
            Group {
                if let worst {
                    // A failed read keeps the last numbers, so they are marked old like a stale meter.
                    Text("\(worst.window.isFresh && model.readError == nil ? "" : "~")\(worst.window.usedPct)%")
                        .foregroundStyle(theme.color(worst.window.severity))
                } else {
                    Text("—").foregroundStyle(Palette.secondary)
                }
            }
            .font(.system(size: 12, weight: .semibold).monospacedDigit())
            .lineLimit(1)
            .frame(width: Layout.wing - 10, alignment: .trailing)
            .padding(.trailing, 10)
        }
        .frame(maxHeight: .infinity)
        .background(alignment: .bottom) { wash }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(worst.map { "\($0.account.name) \($0.window.kind.title) \($0.window.usedPct) percent used" } ?? "No usage reading")
        .accessibilityAddTraits(.isButton)
        .accessibilityHint(model.hotKeyReady ? "Opens the usage card. Control Option Command U also opens it." : "Opens the usage card.")
        .accessibilityAction { model.toggleFromKeyboard?() }
    }

    /// The phase cue inside each wing's bottom 8 pt; the camera's width stays black.
    private var wash: some View {
        HStack(spacing: 0) {
            LinearGradient(colors: [.clear, theme.bloom.opacity(0.35)], startPoint: .top, endPoint: .bottom).frame(width: Layout.wing)
            Spacer(minLength: 0)
            LinearGradient(colors: [.clear, theme.bloom.opacity(0.35)], startPoint: .top, endPoint: .bottom).frame(width: Layout.wing)
        }
        .frame(height: 8)
    }
}

struct OpenContent: View {
    let model: UsageModel
    let theme: Theme
    let now: Date
    let live: Bool

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let kinds = WindowKind.allCases

    var body: some View {
        VStack(spacing: 0) {
            status.frame(height: Layout.status)
            HStack(spacing: Layout.gap) {
                Spacer().frame(width: Layout.markColumn + Layout.gap + Layout.nameColumn)
                ForEach(kinds, id: \.self) { kind in
                    Text(kind.title).frame(width: Layout.windowColumn)
                }
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(Palette.tertiary)
            .frame(height: Layout.headings, alignment: .top)
            // ImageRenderer draws no ScrollView content, so a snapshot shows the rows that fit.
            if live {
                ScrollViewReader { scroller in
                    ScrollView(.vertical) { rows }
                        // Opening the fold adds rows below the live accounts; bring the first into view.
                        .onChange(of: model.foldOpen) { _, open in
                            guard open, let first = model.signedOut.first else { return }
                            withAnimation(reduceMotion ? nil : .easeOut(duration: 0.25)) {
                                scroller.scrollTo(first.id, anchor: .top)
                            }
                        }
                }
            } else {
                Color.clear.overlay(alignment: .top) { rows }.clipped()
            }
            if !model.signedOut.isEmpty { fold }
        }
        .padding(.horizontal, 20)
        .padding(.bottom, Layout.bottom)
    }

    private var rows: some View {
        VStack(spacing: 0) {
            ForEach(model.liveAccounts) { account in
                AccountRow(account: account, kinds: kinds, theme: theme, now: now)
            }
            if model.foldOpen {
                ForEach(model.signedOut) { account in
                    SignedOutRow(account: account).id(account.id)
                }
            }
        }
    }

    private var status: some View {
        HStack(spacing: 8) {
            if let worst = worstWindow(model.accounts) {
                Circle().fill(theme.color(worst.window.severity)).frame(width: 7, height: 7)
                // A long name gives way; the window and its value always show.
                HStack(spacing: 0) {
                    Text(worst.account.name).lineLimit(1).truncationMode(.middle)
                    Text(" · \(worst.window.kind.title) · \(worst.window.usedPct)% used").fixedSize()
                }
                .foregroundStyle(Palette.primary)
            } else {
                // The cause names the fix (for example "swarm not found; set SWARM_BIN").
                Text(model.readError.map { "\($0) · retries each minute" } ?? "No usage reading")
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(1).truncationMode(.tail)
                    .help(model.readError ?? "")
            }
            Spacer(minLength: 12)
            if let refreshError = model.refreshError {
                Text(refreshError).foregroundStyle(Palette.warn).lineLimit(1).truncationMode(.tail)
            } else {
                Text(age).foregroundStyle(Palette.tertiary).monospacedDigit()
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
        .font(.system(size: 12, weight: .medium))
    }

    private var age: String {
        if model.readError != nil { return "read failed" }
        guard let readAt = model.readAt else { return "reading…" }
        let minutes = Int(now.timeIntervalSince(readAt) / 60)
        return minutes < 1 ? "updated now" : "updated \(minutes)m ago"
    }

    private var fold: some View {
        Button { model.foldOpen.toggle() } label: {
            HStack(spacing: 6) {
                Image(systemName: model.foldOpen ? "chevron.down" : "chevron.right").font(.system(size: 9, weight: .semibold))
                let providers = Set(model.signedOut.map(\.provider))
                let who = providers.count == 1 ? (providers.first == "claude" ? "Claude " : "Codex ") : ""
                Text("\(model.signedOut.count) \(who)\(model.signedOut.count == 1 ? "account needs" : "accounts need") sign-in")
                Spacer()
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(Palette.tertiary)
            .frame(height: Layout.fold)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(model.foldOpen ? "expanded" : "collapsed")
        .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
    }
}

struct AccountRow: View {
    let account: UsageAccount
    let kinds: [WindowKind]
    let theme: Theme
    let now: Date

    var body: some View {
        HStack(spacing: Layout.gap) {
            // The subtitle already names the provider, so the mark is not read twice.
            ProviderMark(provider: account.provider, size: 16).frame(width: Layout.markColumn).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(account.name).font(.system(size: 13, weight: .medium)).foregroundStyle(Palette.primary)
                    .lineLimit(1).truncationMode(.middle).help(account.name)
                Text(subtitle).font(.system(size: 11)).foregroundStyle(Palette.secondary).lineLimit(1)
            }
            .frame(width: Layout.nameColumn, alignment: .leading)
            ForEach(kinds, id: \.self) { kind in
                if let window = account.windows.first(where: { $0.kind == kind }) {
                    WindowCell(window: window, theme: theme, now: now)
                } else {
                    Text("—").font(.system(size: 12)).foregroundStyle(Palette.tertiary).frame(width: Layout.windowColumn)
                }
            }
        }
        .accessibilityElement(children: .combine)
        .frame(height: Layout.row)
        .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
    }

    private var subtitle: String {
        let provider = account.provider == "claude" ? "Claude" : "Codex"
        let stale = account.windows.filter { !$0.isFresh }.compactMap(\.asOf).min()
        guard let stale else { return provider }
        // shortDuration says "now" under a minute, which reads wrong for an age.
        let age = now.timeIntervalSince(stale) < 60 ? "<1m" : shortDuration(until: now, now: stale)
        return "\(provider) · \(age) old"
    }
}

struct WindowCell: View {
    let window: UsageWindow
    let theme: Theme
    let now: Date

    var body: some View {
        let color = theme.color(window.severity)
        VStack(spacing: 3) {
            ZStack {
                Ring(fraction: Double(window.usedPct) / 100, color: color)
                Text("\(window.usedPct)")
                    .font(.system(size: 11.5, weight: .semibold).monospacedDigit())
                    .foregroundStyle(color)
            }
            .frame(width: 32, height: 32)
            .overlay(alignment: .topTrailing) {
                if window.severity == .critical {
                    Text("!").font(.system(size: 8, weight: .heavy)).foregroundStyle(Palette.critical).offset(x: 6, y: -1)
                }
            }
            Text(window.resetAt.map { shortDuration(until: $0, now: now) } ?? "—")
                .font(.system(size: 11).monospacedDigit())
                .foregroundStyle(Palette.tertiary)
                .frame(height: 13)
        }
        .frame(width: Layout.windowColumn)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(window.kind.title) \(window.usedPct) percent used"
            + (window.severity == .critical ? ", critical" : "")
            + (window.resetAt.map { ", resets in \(shortDuration(until: $0, now: now))" } ?? ""))
    }
}

/// A 270° gauge, open at the bottom, with a tick at 80%. Zero draws no arc.
struct Ring: View {
    let fraction: Double
    let color: Color

    var body: some View {
        ZStack {
            Circle().trim(from: 0, to: 0.75)
                .stroke(Palette.track, style: .init(lineWidth: 3, lineCap: .round))
            if fraction > 0 {
                Circle().trim(from: 0, to: 0.75 * min(fraction, 1))
                    .stroke(color, style: .init(lineWidth: 3, lineCap: .round))
            }
            Circle().trim(from: 0.75 * 0.8 - 0.004, to: 0.75 * 0.8 + 0.004)
                .stroke(Color.black.opacity(0.9), style: .init(lineWidth: 3))
        }
        .rotationEffect(.degrees(135))
        .padding(1.5)
    }
}

struct SignedOutRow: View {
    let account: UsageAccount

    var body: some View {
        HStack(spacing: Layout.gap) {
            ProviderMark(provider: account.provider, size: 16).frame(width: Layout.markColumn)
            Text(account.name).font(.system(size: 13, weight: .medium)).foregroundStyle(Palette.secondary)
                .lineLimit(1).truncationMode(.middle).help(account.name)
                .frame(width: Layout.nameColumn, alignment: .leading)
            Text(account.problem ?? "No reading").font(.system(size: 11)).foregroundStyle(Palette.tertiary)
                .lineLimit(2).truncationMode(.tail).help(account.problem ?? "")
            Spacer()
        }
        .accessibilityElement(children: .combine)
        .frame(height: Layout.signedOutRow)
        .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
    }
}
