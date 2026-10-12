import Foundation
import Observation

/// The one-word reading in the HUD header for the whole account list.
public enum UsageMood: Sendable, Equatable, CaseIterable {
    case calm, warn, danger, asleep, thinking

    public var caption: String {
        switch self {
        case .calm: "All calm"
        case .warn: "Running warm"
        case .danger: "Ease off"
        case .asleep: "No fresh usage"
        case .thinking: "Fetching usage…"
        }
    }
}

/// One line under a meter bar.
public struct UsageDetailLine: Sendable, Equatable {
    public let text: String
    /// The line that carries the severity color.
    public let risk: Bool
    /// What a narrow column shows in place of `text`.
    public let short: String

    public init(text: String, risk: Bool, short: String? = nil) {
        self.text = text
        self.risk = risk
        self.short = short ?? text
    }
}

public struct UsageHUDNotice: Sendable, Equatable {
    public let text: String
    public let needsSignIn: Bool
}

public struct UsageHUDAccount: Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let meters: [SwarmUsageMeter]

    /// An account with no usage window shows this text in place of its meters.
    public var notice: UsageHUDNotice? {
        guard let meter = meters.first(where: { $0.window == nil }) else { return nil }
        // Open Accounts can fix only a missing or refused sign-in; an expired token renews in a chat.
        switch meter.knownReasonCode {
        case .reloginRequired, .noCredentials:
            return UsageHUDNotice(text: "Sign in to this account to see usage.", needsSignIn: true)
        default:
            return UsageHUDNotice(text: meter.reasonText ?? "No usage reading", needsSignIn: false)
        }
    }
}

/// One meter column of a provider card: a usage window such as 5 hours.
public struct UsageHUDColumn: Sendable, Equatable, Identifiable {
    /// The `window` value of the meters in this column.
    public let id: String
    public let title: String
}

public struct UsageHUDSection: Sendable, Equatable, Identifiable {
    public var id: String { provider }
    public let provider: String
    public let title: String
    public let windows: [UsageHUDColumn]
    public let accounts: [UsageHUDAccount]
}

/// The one-time question at app start. The answer is `Prefs.usageHUD`.
public enum UsageHUDPrompt {
    public static let title = "Add the usage HUD?"
    public static let message = "Swarm can show Claude and Codex usage in a panel at the top of the screen. Hover over the notch to open it. You can turn it off in Settings › Accounts."
    public static let acceptTitle = "Add HUD"
    public static let declineTitle = "No Thanks"

    /// The setup sheet goes first, so the two never show together.
    public static func shouldAsk(answer: Bool?, askedThisRun: Bool, setupSheetOpen: Bool) -> Bool {
        answer == nil && !askedThisRun && !setupSheetOpen
    }

    /// Read from the choices file, because Swarm starts at login with no window (ADR 0069) and
    /// the HUD cannot wait for one. A file that cannot be read shows no HUD.
    @MainActor
    public static func showsAtLaunch(choices: OwnerChoicesStore) -> Bool {
        (try? choices.load(waitForLock: true).prefs.usageHUD) == true
    }
}

/// Sizes and rects of the HUD panel. A rect has its origin at the bottom left, as AppKit has.
public enum UsageHUDLayout {
    /// The collapsed size on a Mac with no notch.
    public static let fallbackNotchSize = CGSize(width: 224, height: 38)
    public static let minPanelWidth: CGFloat = 584
    /// Wide enough for the Claude and Codex cards side by side.
    public static let defaultPanelWidth: CGFloat = 1_100
    /// Room around the card inside the window for its shadow.
    public static let panelWindowMargin: CGFloat = 40
    public static let minPanelHeightLimit: CGFloat = 240
    /// The design tokens of the provider card take these two, so the drawn card and
    /// `providerCardMinWidth` cannot disagree.
    public static let cardPaddingHorizontal: CGFloat = 12
    public static let columnSpacing: CGFloat = 14
    /// The account column and one meter at the width where two cards first sit side by side. The
    /// account column takes a share of the card, so here it is wider than its smallest drawn width.
    private static let sideBySideAccountWidth: CGFloat = 150
    private static let sideBySideMeterWidth: CGFloat = 112
    private static let notchHoverHorizontalInset: CGFloat = 8
    private static let notchHoverBottomInset: CGFloat = 5
    private static let notchHoverTopInset: CGFloat = 1

    /// The open panel grows inside the visible height of its screen. Only a longer list scrolls.
    public static func panelHeight(screenHeight: CGFloat) -> CGFloat { max(400, screenHeight - 40) }

    public static func maxPanelWidth(screenWidth: CGFloat) -> CGFloat {
        max(minPanelWidth, (screenWidth * 0.9).rounded(.down))
    }

    /// The card width for this screen. nil or NaN takes the default.
    public static func clampedPanelWidth(_ stored: Double?, screenWidth: CGFloat) -> CGFloat {
        let width = stored.flatMap { $0.isFinite ? CGFloat($0) : nil } ?? defaultPanelWidth
        return min(max(width, minPanelWidth), maxPanelWidth(screenWidth: screenWidth))
    }

    /// The part of the window that shows content: the notch when collapsed, the card when open.
    /// Both attach to the top edge of the window.
    public static func contentRect(
        isCollapsed: Bool, expandedHeight: CGFloat, expandedWidth: CGFloat,
        notchWidth: CGFloat, notchHeight: CGFloat, panelSize: CGSize
    ) -> CGRect {
        let size = isCollapsed
            ? CGSize(width: notchWidth, height: notchHeight)
            : CGSize(width: expandedWidth, height: notchHeight + expandedHeight)
        return CGRect(
            x: (panelSize.width - size.width) / 2, y: panelSize.height - size.height,
            width: size.width, height: size.height
        )
    }

    public static func physicalNotchRect(panelFrame: CGRect, notchWidth: CGFloat, notchHeight: CGFloat) -> CGRect {
        CGRect(x: panelFrame.midX - notchWidth / 2, y: panelFrame.maxY - notchHeight,
               width: notchWidth, height: notchHeight)
    }

    /// The notch plus its shoulders and the top edge of the screen.
    public static func fullNotchHoverRect(_ notch: CGRect) -> CGRect {
        let sideWidth = max(0, notch.height - 12) + 24
        return CGRect(
            x: notch.midX - (notch.width + sideWidth) / 2 - notchHoverHorizontalInset,
            y: notch.minY - notchHoverBottomInset,
            width: notch.width + sideWidth + notchHoverHorizontalInset * 2,
            height: notch.height + notchHoverBottomInset + notchHoverTopInset
        )
    }

    public static func topEdgeInclusiveRect(_ rect: CGRect) -> CGRect {
        CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: rect.height + notchHoverTopInset)
    }

    public static func notchAnchoredOrigin(size: CGSize, screenFrame: CGRect) -> CGPoint {
        CGPoint(x: screenFrame.midX - size.width / 2, y: screenFrame.maxY - size.height)
    }

    /// Side by side when every card gets its minimum width; spare width splits in proportion to
    /// the minimums. nil means the cards stack.
    public static func providerCardWidths(minWidths: [CGFloat], total: CGFloat, gap: CGFloat) -> [CGFloat]? {
        guard minWidths.count > 1 else { return nil }
        let gaps = gap * CGFloat(minWidths.count - 1)
        let needed = minWidths.reduce(0, +)
        guard total - gaps >= needed else { return nil }
        return minWidths.map { ($0 * (total - gaps) / needed).rounded(.down) }
    }

    /// Card padding, the account column, and one meter per column with its spacing.
    public static func providerCardMinWidth(columns: Int) -> CGFloat {
        2 * cardPaddingHorizontal + sideBySideAccountWidth
            + CGFloat(columns) * (sideBySideMeterWidth + columnSpacing)
    }
}

/// What the usage HUD shows and the state of its panel. It reads `swarm usage --json`; only
/// `refresh()` asks the providers, through the shared refresher.
@MainActor @Observable
public final class UsageHUDModel {
    public private(set) var meters: [SwarmUsageMeter] = []
    public private(set) var pressures: [UsageRowPressure] = []
    /// When the shown meters were read. A run-out time counts from this moment.
    public private(set) var readAtSeconds: Int?
    public private(set) var readFailed = false
    public private(set) var hasLoaded = false
    public private(set) var isRefreshing = false
    private var refreshFailure: String?
    /// Set when a Refresh made no call because the 300-second limit was not over.
    private var cooldownEndSeconds: Int?

    /// A hover opens the panel for a short time. The menu opens it until the owner closes it.
    public var isCollapsed = true
    public var openedFromMenu = false
    /// True while an edge is dragged. The panel must not collapse then.
    public var isResizing = false
    /// The size of the window. The visible card is smaller and sits at its top.
    public private(set) var panelSize = CGSize(width: 624, height: UsageHUDLayout.panelHeight(screenHeight: 800))
    /// The measured height of the open card, at most the window height.
    public var expandedContentHeight: CGFloat
    /// What the card needs to show every account, with its header and footer.
    @ObservationIgnored private var neededContentHeight: CGFloat?
    public private(set) var panelWidth = UsageHUDLayout.minPanelWidth
    /// The most height the card may take, set with its bottom edge. nil fits every account.
    public private(set) var panelHeightLimit: CGFloat?
    /// False on a screen with no notch. The collapsed panel is then a pill on the menu bar.
    public var hasNotch = true
    public var notchTopInset = UsageHUDLayout.fallbackNotchSize.height
    public var notchTriggerWidth = UsageHUDLayout.fallbackNotchSize.width

    private static let panelWidthKey = "usageHUDPanelWidthPoints"
    private static let panelHeightLimitKey = "usageHUDPanelHeightLimitPoints"
    private let loadUsage: @Sendable () async throws -> [SwarmUsageMeter]
    private let refresher: UsageRefresher
    private let history: UsageHistoryStore
    private let defaults: UserDefaults
    private let now: @Sendable () -> Int
    @ObservationIgnored private var isReading = false
    @ObservationIgnored private var readQueued = false
    @ObservationIgnored private var pendingCeilingReset = false
    @ObservationIgnored private var screenWidth: CGFloat = 0

    public init(
        loadUsage: @escaping @Sendable () async throws -> [SwarmUsageMeter],
        refresher: UsageRefresher,
        history: UsageHistoryStore = UsageHistoryStore(),
        defaults: UserDefaults = .standard,
        now: @escaping @Sendable () -> Int = { Int(Date().timeIntervalSince1970) }
    ) {
        self.loadUsage = loadUsage
        self.refresher = refresher
        self.history = history
        self.defaults = defaults
        self.now = now
        expandedContentHeight = UsageHUDLayout.panelHeight(screenHeight: 800)
        panelHeightLimit = (defaults.object(forKey: Self.panelHeightLimitKey) as? Double)
            .flatMap { $0.isFinite ? max(UsageHUDLayout.minPanelHeightLimit, CGFloat($0)) : nil }
    }

    // MARK: Reads

    /// Reads the cached usage. A read that arrives during another read runs once after it.
    public func read() async {
        if isReading {
            readQueued = true
            return
        }
        isReading = true
        defer { isReading = false }
        repeat {
            readQueued = false
            do {
                let meters = try await loadUsage()
                let nowSeconds = now()
                let history = history
                let samples = UsageHistoryStore.samples(from: meters, nowSeconds: nowSeconds)
                let all = await Task.detached { history.append(samples, nowSeconds: nowSeconds) }.value
                self.meters = meters
                pressures = UsagePressure.rows(meters: meters, history: all, nowSeconds: nowSeconds)
                readAtSeconds = nowSeconds
                readFailed = false
                hasLoaded = true
            } catch {
                // The last good meters stay on screen.
                readFailed = true
            }
        } while readQueued
    }

    /// The owner's Refresh. Inside the 300-second limit it makes no provider call and reads the cache.
    public func refresh() async {
        guard !isRefreshing else { return }
        isRefreshing = true
        defer { isRefreshing = false }
        refreshFailure = nil
        cooldownEndSeconds = nil
        let refresher = refresher
        let results = await withTaskGroup(of: (provider: String, nextInSeconds: Int?, failure: String?).self) { group in
            for provider in AccountsPageModel.usageProviders {
                group.addTask {
                    do {
                        if case .cooldown(let next) = try await refresher.refresh(provider: provider) {
                            return (provider, next, nil)
                        }
                        return (provider, nil, nil)
                    } catch {
                        return (provider, nil, AccountsPageModel.message(error))
                    }
                }
            }
            var results: [(provider: String, nextInSeconds: Int?, failure: String?)] = []
            for await result in group { results.append(result) }
            return results
        }
        let failures = AccountsPageModel.usageProviders.compactMap { provider in
            results.first { $0.provider == provider }?.failure.map {
                "\(AccountsPageModel.providerTitle(provider)): \($0)"
            }
        }
        refreshFailure = failures.isEmpty ? nil : failures.joined(separator: " ")
        cooldownEndSeconds = results.compactMap(\.nextInSeconds).max().map { now() + $0 }
        await read()
    }

    // MARK: What the panel shows

    /// The first read failed, so there is nothing to show.
    public var firstReadFailed: Bool { meters.isEmpty && readFailed }
    public var hasOlderSamples: Bool { meters.contains { $0.state == .stale } }
    public var accountCount: Int { sections.reduce(0) { $0 + $1.accounts.count } }

    public var sections: [UsageHUDSection] {
        AccountsPageModel.usageProviders.compactMap { provider in
            let meters = meters.filter { $0.provider == provider }
            guard !meters.isEmpty else { return nil }
            var accounts: [UsageHUDAccount] = []
            for meter in meters where !accounts.contains(where: { $0.name == meter.account ?? meter.label }) {
                let name = meter.account ?? meter.label
                accounts.append(UsageHUDAccount(
                    id: "\(provider):\(name)", name: name,
                    meters: meters.filter { ($0.account ?? $0.label) == name }
                ))
            }
            var minutes: [String: Int] = [:]
            for meter in meters {
                guard let window = meter.window else { continue }
                minutes[window] = minutes[window] ?? meter.windowMinutes ?? .max
            }
            let windows = minutes.sorted { ($0.value, $0.key) < ($1.value, $1.key) }.map {
                UsageHUDColumn(id: $0.key, title: Self.windowTitle(window: $0.key, minutes: $0.value))
            }
            return UsageHUDSection(
                provider: provider, title: AccountsPageModel.providerTitle(provider), windows: windows, accounts: accounts
            )
        }
    }

    public var mood: UsageMood {
        Self.mood(hasLoaded: hasLoaded, isRefreshing: isRefreshing, meters: meters, pressures: pressures)
    }

    public var headerCaption: String {
        let count = accountCount
        let accounts = count == 0 ? "Claude & Codex" : CountText.count(count, singular: "account", plural: "accounts")
        return "\(accounts) · \(mood.caption)"
    }

    /// The worst reading, for the collapsed pill on a Mac with no notch.
    public var pill: (usedPct: Int, severity: UsageSeverity)? {
        guard let binding = UsagePressure.binding(pressures), let basis = UsagePressure.basis(binding) else { return nil }
        return (Int(binding.usedPct.rounded()), basis.severity)
    }

    public func footer(nowSeconds: Int) -> (text: String, warning: Bool) {
        if readFailed { return ("Swarm could not read usage. Click Refresh to try again.", true) }
        if let refreshFailure { return ("Last refresh: \(refreshFailure)", true) }
        if let text = cooldownEndSeconds.flatMap({ Self.cooldownText(endSeconds: $0, nowSeconds: nowSeconds) }) {
            return (text, false)
        }
        return (hasOlderSamples ? "Clock marks an older sample." : "Usage from swarm", false)
    }

    /// After a Refresh that made no provider call, until the next call may run.
    public nonisolated static func cooldownText(endSeconds: Int, nowSeconds: Int) -> String? {
        guard nowSeconds < endSeconds else { return nil }
        let updated = ageLabel(sinceSeconds: endSeconds - UsageRefresher.minIntervalSeconds, nowSeconds: nowSeconds)
        return "Updated \(updated). Next refresh in \((endSeconds - nowSeconds + 59) / 60)m."
    }

    /// A refresh outranks every reading, because the owner just started it. With no fresh meter
    /// there is nothing to judge, so the mood is asleep and not calm.
    public nonisolated static func mood(
        hasLoaded: Bool, isRefreshing: Bool, meters: [SwarmUsageMeter], pressures: [UsageRowPressure]
    ) -> UsageMood {
        if isRefreshing { return .thinking }
        guard hasLoaded, meters.contains(where: { $0.state == .fresh }) else { return .asleep }
        switch UsagePressure.basis(UsagePressure.binding(pressures))?.severity {
        case .danger: return .danger
        case .warn: return .warn
        case .calm, nil: return .calm
        }
    }

    /// A Codex limit name comes before the colon and stays in the title.
    public nonisolated static func windowTitle(window: String, minutes: Int?) -> String {
        let parts = window.split(separator: ":", omittingEmptySubsequences: false).map(String.init)
        let name = parts.last ?? ""
        let title: String = switch (name, minutes) {
        case ("fb", _): "Fable · week"
        case (_, UsagePressure.fiveHourMinutes): "5 hours"
        case (_, UsagePressure.weekMinutes): "Weekly"
        default: name.isEmpty ? "—" : name
        }
        let limit = parts.dropLast().joined(separator: ":")
        return limit.isEmpty ? title : "\(limit) · \(title)"
    }

    nonisolated static func ageLabel(sinceSeconds: Int, nowSeconds: Int) -> String {
        let seconds = max(0, nowSeconds - sinceSeconds)
        if seconds < 60 { return "just now" }
        if seconds < 3_600 { return "\(seconds / 60)m ago" }
        if seconds < 86_400 { return "\(seconds / 3_600)h ago" }
        return "\(seconds / 86_400)d ago"
    }

    /// An old value explains its age and the action that gets a new one.
    public nonisolated static func freshnessDetail(_ meter: SwarmUsageMeter, nowSeconds: Int) -> String? {
        guard meter.state != .fresh else { return nil }
        let source = meter.source.map(AccountsPageModel.sourceLabel)
        if meter.state == .stale, let reset = meter.resetTimeSeconds, reset <= nowSeconds {
            return "Window reset · waiting for a current \(source.map { "\($0) " } ?? "")sample"
        }
        let action = " · click Refresh to fetch current usage"
        guard meter.usedPct != nil else { return "No sample for this window" + action }
        guard let asOf = meter.asOfSeconds else { return nil }
        let age = ageLabel(sinceSeconds: asOf, nowSeconds: nowSeconds)
        return (source.map { "\($0) last confirmed \(age)" } ?? "Last confirmed \(age)") + action
    }

    /// The line under an account name. A failed refresh leads, because the owner can act on it.
    /// The age is that of the oldest sample of the account.
    public nonisolated static func accountSubtitle(
        _ meters: [SwarmUsageMeter], nowSeconds: Int
    ) -> (text: String, warning: Bool) {
        let age = meters.compactMap(\.asOfSeconds).min().map { ageLabel(sinceSeconds: $0, nowSeconds: nowSeconds) }
        let failed = meters.contains {
            [.failed, .unavailable].contains($0.state) || ($0.state == .stale && $0.reason != nil)
        }
        return failed
            ? (["Refresh failed", age].compactMap { $0 }.joined(separator: " · "), true)
            : (age ?? "", false)
    }

    /// "14:20" today, "Sun 10:41" on another day, or "2:20 PM" with a 12-hour region. A time with
    /// no weekday reads as today.
    public nonisolated static func limitTimeLabel(
        _ seconds: Int, nowSeconds: Int, calendar: Calendar = .current
    ) -> String {
        let date = Date(timeIntervalSince1970: Double(seconds))
        let sameDay = calendar.isDate(date, inSameDayAs: Date(timeIntervalSince1970: Double(nowSeconds)))
        let style = Date.FormatStyle(locale: calendar.locale ?? .current, calendar: calendar, timeZone: calendar.timeZone)
            .hour().minute()
        return date.formatted(sameDay ? style : style.weekday(.abbreviated))
    }

    /// "45m", "1h 10m", "2d 3h".
    public nonisolated static func gapLabel(hours: Double) -> String {
        let minutes = max(0, Int((hours * 60).rounded()))
        if minutes < 60 { return "\(minutes)m" }
        if minutes < 24 * 60 {
            return minutes % 60 == 0 ? "\(minutes / 60)h" : "\(minutes / 60)h \(minutes % 60)m"
        }
        let days = minutes / (24 * 60), hours = minutes % (24 * 60) / 60
        return hours == 0 ? "\(days)d" : "\(days)d \(hours)h"
    }

    /// The lines under a meter bar. The reset time always shows. A risk adds the run-out time (red)
    /// or the projected use at reset (amber).
    public nonisolated static func cellDetail(
        meter: SwarmUsageMeter, pressure: UsageRowPressure?, readAtSeconds: Int?, nowSeconds: Int,
        calendar: Calendar = .current
    ) -> [UsageDetailLine] {
        guard let resetAt = meter.resetTimeSeconds else { return [] }
        let reset: String
        if resetAt <= nowSeconds {
            // An old sample of a window that has reset has no next reset to show.
            guard meter.state != .stale else { return [] }
            reset = "Resets now"
        } else {
            reset = "Resets \(limitTimeLabel(resetAt, nowSeconds: nowSeconds, calendar: calendar))"
        }
        let plain = [UsageDetailLine(text: reset, risk: false)]
        guard meter.state == .fresh, let pressure, let readAtSeconds else { return plain }
        switch pressure.level {
        case .red:
            guard let hoursToFull = pressure.hoursToFull else { return plain }
            let runsOutAt = Double(readAtSeconds) + hoursToFull * 3_600
            // The labels show whole minutes, so the gap is between those minutes and always
            // agrees with the two times on screen.
            let gapMinutes = (Double(resetAt) / 60).rounded(.down) - (runsOutAt / 60).rounded(.down)
            let runsOut = limitTimeLabel(Int(runsOutAt), nowSeconds: nowSeconds, calendar: calendar)
            return [
                UsageDetailLine(text: "Runs out \(runsOut)", risk: true, short: "Out \(runsOut)"),
                UsageDetailLine(
                    text: gapMinutes < 1 ? reset : "\(reset) · \(gapLabel(hours: gapMinutes / 60)) later",
                    risk: false, short: reset
                ),
            ]
        case .amber:
            guard let projected = pressure.projectedPct else { return plain }
            let percent = min(100, Int(projected.rounded()))
            return plain + [UsageDetailLine(text: "On pace for \(percent)% by then", risk: true, short: "\(percent)% by reset")]
        case .green:
            return plain
        }
    }

    // MARK: Panel size

    /// Sets the window height for a screen. The measured card height resets to it only while the
    /// panel is collapsed; a change that arrives while the card is open waits for the next
    /// collapsed call, so the open card never jumps.
    public func applyPanelCeiling(_ height: CGFloat) {
        if height != panelSize.height {
            panelSize.height = height
            pendingCeilingReset = true
        }
        guard pendingCeilingReset, isCollapsed else { return }
        pendingCeilingReset = false
        expandedContentHeight = height
    }

    /// Clamps the saved width for this screen. The saved value does not change, so a wider width
    /// returns on a wider screen.
    public func applyScreenWidth(_ width: CGFloat) {
        screenWidth = width
        panelSize.width = UsageHUDLayout.maxPanelWidth(screenWidth: width) + UsageHUDLayout.panelWindowMargin
        panelWidth = UsageHUDLayout.clampedPanelWidth(defaults.object(forKey: Self.panelWidthKey) as? Double, screenWidth: width)
    }

    /// A drag moves the width. Only the end of the drag saves it.
    public func resizePanel(to width: CGFloat) {
        panelWidth = UsageHUDLayout.clampedPanelWidth(Double(width), screenWidth: screenWidth)
    }

    public func savePanelWidth() {
        defaults.set(Double(panelWidth), forKey: Self.panelWidthKey)
    }

    /// The card measured its content. The open height is that, inside the limit and the screen.
    public func contentNeeds(height: CGFloat) {
        neededContentHeight = height
        fitContentHeight()
    }

    /// The backdrop, the hover area and the next drag start follow the new limit at once.
    public func resizePanelHeight(to height: CGFloat) {
        panelHeightLimit = max(UsageHUDLayout.minPanelHeightLimit, height)
        fitContentHeight()
    }

    private func fitContentHeight() {
        expandedContentHeight = min(
            neededContentHeight ?? expandedContentHeight,
            panelHeightLimit ?? .infinity, max(0, panelSize.height - notchTopInset)
        )
    }

    public func savePanelHeightLimit() {
        defaults.set(panelHeightLimit.map(Double.init), forKey: Self.panelHeightLimitKey)
    }
}
