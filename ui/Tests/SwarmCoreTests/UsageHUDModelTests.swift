import Foundation
import Testing
@testable import SwarmCore

/// Monday 12 October 2026, 10:00 UTC. Same-day times carry no weekday.
private let now = 1_791_799_200

private var utc: Calendar {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "UTC")!
    calendar.locale = Locale(identifier: "en_GB")
    return calendar
}

private func meter(
    provider: String = "claude", account: String = "work", window: String? = "5h", minutes: Int? = 300,
    usedPct: Double? = 60, resetInHours: Double? = 2, state: SwarmUsageState = .fresh,
    source: String? = "claude_api", reason: String? = nil, reasonCode: String? = nil, asOfSecondsAgo: Int? = 0
) -> SwarmUsageMeter {
    SwarmUsageMeter(provider: provider, account: account, label: account, window: window, windowMinutes: minutes,
                    usedPct: usedPct, resetTimeSeconds: resetInHours.map { now + Int($0 * 3_600) }, state: state,
                    source: source, reason: reason, reasonCode: reasonCode, asOfSeconds: asOfSecondsAgo.map { now - $0 })
}

private func pressure(
    _ level: UsagePressureLevel, usedPct: Double = 60, burn: Double? = 30, hoursToFull: Double? = nil,
    projected: Double? = nil
) -> UsageRowPressure {
    UsageRowPressure(meterID: "claude:work:5h", usedPct: usedPct, burnPctPerHour: burn, hoursToReset: 2,
                     projectedPct: projected, hoursToFull: hoursToFull, level: level)
}

/// Stands in for `swarm usage --json` and for the refresh command.
private actor UsageCommand {
    var meters: [SwarmUsageMeter]
    private(set) var refreshed: [String] = []
    private var readFails = false
    private var refreshFailures: [String: String] = [:]

    init(_ meters: [SwarmUsageMeter] = []) { self.meters = meters }

    func set(_ meters: [SwarmUsageMeter]) { self.meters = meters }
    func failRead(_ fails: Bool) { readFails = fails }
    func failRefresh(_ provider: String, _ message: String) { refreshFailures[provider] = message }

    func read() throws -> [SwarmUsageMeter] {
        if readFails { throw SwarmProfileError.failed("swarm usage failed") }
        return meters
    }

    func refresh(_ provider: String) throws -> SwarmUsage {
        refreshed.append(provider)
        if let message = refreshFailures[provider] { throw SwarmProfileError.failed(message) }
        return SwarmUsage(meters: meters.filter { $0.provider == provider })
    }
}

/// Holds the first read until `release`, as a poll read still in flight.
private actor HeldRead {
    private var first = true
    private var released = false
    private var waiter: CheckedContinuation<Void, Never>?
    private(set) var isHolding = false

    /// True for the first call only, after the hold ends.
    func holdFirst() async -> Bool {
        guard first else { return false }
        first = false
        guard !released else { return true }
        isHolding = true
        await withCheckedContinuation { waiter = $0 }
        return true
    }

    func release() {
        released = true
        waiter?.resume()
        waiter = nil
    }
}

@Suite("Usage HUD model")
@MainActor
struct UsageHUDModelTests {
    private func makeModel(
        _ command: UsageCommand, clock: UsageTestClock = UsageTestClock(now), defaults: UserDefaults? = nil
    ) throws -> UsageHUDModel {
        let suite = "UsageHUDModelTests.\(UUID().uuidString)"
        return UsageHUDModel(
            loadUsage: { try await command.read() },
            refresher: UsageRefresher(now: clock.now) { try await command.refresh($0) },
            history: UsageHistoryStore(folder: nil),
            defaults: try defaults ?? #require(UserDefaults(suiteName: suite)),
            now: clock.now
        )
    }

    // MARK: Mood

    @Test("the mood sleeps with no fresh meter, thinks during a refresh, and else follows the worst row")
    func mood() {
        func mood(loaded: Bool = true, refreshing: Bool = false, meters: [SwarmUsageMeter], _ pressures: [UsageRowPressure]) -> UsageMood {
            UsageHUDModel.mood(hasLoaded: loaded, isRefreshing: refreshing, meters: meters, pressures: pressures)
        }
        #expect(mood(loaded: false, meters: [], []) == .asleep)
        #expect(mood(meters: [meter(state: .stale)], []) == .asleep)
        // A refresh outranks every reading, also one that is at danger.
        #expect(mood(refreshing: true, meters: [meter(usedPct: 95)], [pressure(.green, usedPct: 95, burn: nil)]) == .thinking)
        #expect(mood(meters: [meter(usedPct: 20)], []) == .calm)
        #expect(mood(meters: [meter()], [pressure(.amber, usedPct: 40, burn: 20)]) == .warn)
        #expect(mood(meters: [meter()], [pressure(.green, usedPct: 75, burn: nil)]) == .warn)
        #expect(mood(meters: [meter()], [pressure(.red, usedPct: 40, burn: 30)]) == .danger)
        #expect(mood(meters: [meter()], [pressure(.green, usedPct: 92, burn: nil)]) == .danger)
    }

    @Test("each mood has its caption")
    func captions() {
        #expect(UsageMood.allCases.map(\.caption) == [
            "All calm", "Running warm", "Ease off", "No fresh usage", "Fetching usage…",
        ])
    }

    // MARK: Lines under a meter

    private func lines(
        _ meter: SwarmUsageMeter, _ pressure: UsageRowPressure?, readAt: Int? = now
    ) -> [UsageDetailLine] {
        UsageHUDModel.cellDetail(meter: meter, pressure: pressure, readAtSeconds: readAt, nowSeconds: now, calendar: utc)
    }

    @Test("a calm meter shows only its reset time")
    func calmDetail() {
        #expect(lines(meter(), pressure(.green)) == [UsageDetailLine(text: "Resets 12:00", risk: false)])
        #expect(lines(meter(), nil) == [UsageDetailLine(text: "Resets 12:00", risk: false)])
        #expect(lines(meter(resetInHours: nil), pressure(.green)).isEmpty)
    }

    @Test("a red meter shows the run-out time, then the reset with the gap between them")
    func redDetail() {
        #expect(lines(meter(), pressure(.red, hoursToFull: 1.3)) == [
            UsageDetailLine(text: "Runs out 11:18", risk: true, short: "Out 11:18"),
            UsageDetailLine(text: "Resets 12:00 · 42m later", risk: false, short: "Resets 12:00"),
        ])
        // 1.295 h is 11:17:42, shown as 11:17, so the gap to 12:00 is 43 minutes.
        #expect(lines(meter(), pressure(.red, hoursToFull: 1.295)).map(\.text) == ["Runs out 11:17", "Resets 12:00 · 43m later"])
        #expect(lines(meter(), pressure(.red, hoursToFull: 1.999)).map(\.text) == ["Runs out 11:59", "Resets 12:00 · 1m later"])
        // Both times show the same minute, so there is no gap to name.
        #expect(lines(meter(), pressure(.red, hoursToFull: 1.9999), readAt: now + 1).map(\.text) == ["Runs out 12:00", "Resets 12:00"])
    }

    @Test("a time on another day carries the weekday")
    func otherDayDetail() {
        #expect(lines(meter(resetInHours: 30), pressure(.red, hoursToFull: 20)).map(\.text) == [
            "Runs out Tue 06:00", "Resets Tue 16:00 · 10h later",
        ])
        #expect(UsageHUDModel.limitTimeLabel(now + 60, nowSeconds: now, calendar: utc) == "10:01")
        #expect(UsageHUDModel.limitTimeLabel(now + 48 * 3_600, nowSeconds: now, calendar: utc) == "Wed 10:00")
    }

    @Test("a limit time follows the region's 12-hour or 24-hour clock")
    func regionClock() {
        var twelve = utc
        twelve.locale = Locale(identifier: "en_US")
        #expect(UsageHUDModel.limitTimeLabel(now + 60, nowSeconds: now, calendar: twelve) == "10:01\u{202F}AM")
        #expect(UsageHUDModel.limitTimeLabel(now + 48 * 3_600, nowSeconds: now, calendar: twelve) == "Wed 10:00\u{202F}AM")
        // The macOS 24-hour setting comes as an `hours` key on the region.
        twelve.locale = Locale(identifier: "en_US@hours=h23")
        #expect(UsageHUDModel.limitTimeLabel(now + 60, nowSeconds: now, calendar: twelve) == "10:01")
    }

    @Test("an amber meter keeps the reset and adds the projection, at most 100%")
    func amberDetail() {
        #expect(lines(meter(), pressure(.amber, projected: 91.6)) == [
            UsageDetailLine(text: "Resets 12:00", risk: false),
            UsageDetailLine(text: "On pace for 92% by then", risk: true, short: "92% by reset"),
        ])
        #expect(lines(meter(), pressure(.amber, projected: 130)).last?.text == "On pace for 100% by then")
    }

    @Test("an old sample, a missing read time and a passed reset show no risk")
    func noRiskDetail() {
        #expect(lines(meter(state: .stale), pressure(.red, hoursToFull: 1.3)).map(\.text) == ["Resets 12:00"])
        #expect(lines(meter(), pressure(.red, hoursToFull: 1.3), readAt: nil).map(\.text) == ["Resets 12:00"])
        #expect(lines(meter(resetInHours: -1), nil).map(\.text) == ["Resets now"])
        #expect(lines(meter(resetInHours: -1, state: .stale), nil).isEmpty)
    }

    @Test("the gap between two times reads in minutes, hours or days")
    func gapLabel() {
        #expect(UsageHUDModel.gapLabel(hours: 0.75) == "45m")
        #expect(UsageHUDModel.gapLabel(hours: 1 + 10.0 / 60) == "1h 10m")
        #expect(UsageHUDModel.gapLabel(hours: 2) == "2h")
        #expect(UsageHUDModel.gapLabel(hours: 51) == "2d 3h")
        #expect(UsageHUDModel.gapLabel(hours: 48) == "2d")
        #expect(UsageHUDModel.gapLabel(hours: -1) == "0m")
    }

    // MARK: Freshness and account lines

    @Test("an old sample names its source, its age and the Refresh action")
    func freshness() {
        #expect(UsageHUDModel.freshnessDetail(meter(), nowSeconds: now) == nil)
        #expect(UsageHUDModel.freshnessDetail(meter(state: .stale, asOfSecondsAgo: 1_200), nowSeconds: now)
            == "Claude API last confirmed 20m ago · click Refresh to fetch current usage")
        let codex = meter(provider: "codex", window: "primary", state: .stale, source: "codex_app_server", asOfSecondsAgo: 7_200)
        #expect(UsageHUDModel.freshnessDetail(codex, nowSeconds: now)
            == "Codex app-server last confirmed 2h ago · click Refresh to fetch current usage")
        // A source that this build does not know shows as its raw text.
        #expect(UsageHUDModel.freshnessDetail(meter(state: .stale, source: "next_source", asOfSecondsAgo: 30), nowSeconds: now)
            == "next_source last confirmed just now · click Refresh to fetch current usage")
        #expect(UsageHUDModel.freshnessDetail(meter(state: .stale, source: nil, asOfSecondsAgo: 4 * 86_400), nowSeconds: now)
            == "Last confirmed 4d ago · click Refresh to fetch current usage")
    }

    @Test("a window with no sample and a window that has reset say so")
    func freshnessWithoutValue() {
        for state in [SwarmUsageState.missing, .failed] {
            #expect(UsageHUDModel.freshnessDetail(meter(usedPct: nil, state: state, asOfSecondsAgo: nil), nowSeconds: now)
                == "No sample for this window · click Refresh to fetch current usage")
        }
        #expect(UsageHUDModel.freshnessDetail(meter(usedPct: 0, resetInHours: -1, state: .stale), nowSeconds: now)
            == "Window reset · waiting for a current Claude API sample")
    }

    @Test("the account line shows the oldest sample age, and a failed refresh leads with a warning")
    func accountSubtitle() {
        let fresh = UsageHUDModel.accountSubtitle([meter(), meter(window: "7d", asOfSecondsAgo: 720)], nowSeconds: now)
        #expect(fresh.text == "12m ago" && !fresh.warning)
        let kept = UsageHUDModel.accountSubtitle(
            [meter(state: .stale, reason: "Usage refresh failed; old reading", asOfSecondsAgo: 4 * 86_400)], nowSeconds: now
        )
        #expect(kept.text == "Refresh failed · 4d ago" && kept.warning)
        let failed = UsageHUDModel.accountSubtitle(
            [meter(window: nil, usedPct: nil, state: .failed, reason: "Sign in again", asOfSecondsAgo: nil)], nowSeconds: now
        )
        #expect(failed.text == "Refresh failed" && failed.warning)
        // An old sample with no failure is only old.
        let old = UsageHUDModel.accountSubtitle([meter(state: .stale, asOfSecondsAgo: 3_600)], nowSeconds: now)
        #expect(old.text == "1h ago" && !old.warning)
    }

    // MARK: Sections

    @Test("meters group by provider and account, with windows in length order")
    func sections() async throws {
        let command = UsageCommand([
            meter(account: "work", window: "fb", minutes: 10_080),
            meter(account: "work", window: "7d", minutes: 10_080),
            meter(account: "work"),
            meter(account: "personal"),
            meter(provider: "codex", account: "main", window: "secondary", minutes: 10_080, source: "codex_app_server"),
            meter(provider: "codex", account: "main", window: "primary", minutes: 300, source: "codex_app_server"),
            SwarmUsageMeter(provider: "agy", account: nil, label: "agy", window: nil, usedPct: nil, state: .noSource),
        ])
        let model = try makeModel(command)
        await model.read()
        #expect(model.sections.map(\.provider) == ["claude", "codex"])
        #expect(model.sections.map(\.title) == ["Claude", "Codex"])
        #expect(model.sections[0].windows.map(\.id) == ["5h", "7d", "fb"])
        #expect(model.sections[0].windows.map(\.title) == ["5 hours", "Weekly", "Fable · week"])
        #expect(model.sections[0].accounts.map(\.name) == ["work", "personal"])
        #expect(model.sections[0].accounts[0].meters.count == 3)
        #expect(model.sections[1].windows.map(\.title) == ["5 hours", "Weekly"])
        #expect(model.accountCount == 3)
        #expect(model.headerCaption == "3 accounts · All calm")
    }

    @Test("a window of unknown length keeps its name, and a Codex limit name stays in the title")
    func windowTitles() {
        #expect(UsageHUDModel.windowTitle(window: "primary", minutes: nil) == "primary")
        #expect(UsageHUDModel.windowTitle(window: "bengalfox:primary", minutes: 300) == "bengalfox · 5 hours")
        #expect(UsageHUDModel.windowTitle(window: "", minutes: nil) == "—")
    }

    @Test("an account with no window shows why, and a signed-out account asks for sign-in")
    func accountNotice() async throws {
        let command = UsageCommand([
            meter(account: "out", window: nil, usedPct: nil, state: .missing, reason: "Account is signed out",
                  reasonCode: "no_credentials", asOfSecondsAgo: nil),
            meter(account: "expired", window: nil, usedPct: nil, state: .failed, reason: "Sign in again",
                  reasonCode: "relogin_required", asOfSecondsAgo: nil),
            meter(account: "broken", window: nil, usedPct: nil, state: .failed, reason: "Claude usage read failed",
                  reasonCode: "read_failed", asOfSecondsAgo: nil),
            meter(account: "work"),
        ])
        let model = try makeModel(command)
        await model.read()
        let accounts = model.sections[0].accounts
        #expect(accounts.map(\.notice?.text) == [
            "Sign in to this account to see usage.", "Sign in to this account to see usage.",
            "Claude usage read failed", nil,
        ])
        #expect(accounts.map(\.notice?.needsSignIn) == [true, true, false, nil])
        #expect(model.sections[0].windows.map(\.id) == ["5h"])
    }

    @Test("each reason code gives its text, only relogin_required and no_credentials offer sign-in, and the text alone decides nothing",
          arguments: [
              ("token_expired", "Token expired", "Token expired. Start a chat on this account to renew it.", false),
              ("relogin_required", "Sign in again", "Sign in to this account to see usage.", true),
              ("no_credentials", "Account is signed out", "Sign in to this account to see usage.", true),
              ("refresh_failed", "Usage refresh failed; old reading", "Usage refresh failed; old reading", false),
              ("no_cached_usage", "No cached usage", "No cached usage", false),
              ("no_source", "API-key mode has no plan-quota source", "API-key mode has no plan-quota source", false),
              ("read_failed", "Claude usage read failed", "Claude usage read failed", false),
              ("no_quota_windows", "No Codex quota windows", "No Codex quota windows", false),
              // A code from a newer swarm, and an old meter with no code: the text shows as it is.
              ("future_code", "Something new", "Something new", false),
              (nil, "Sign in again", "Sign in again", false),
          ] as [(String?, String, String, Bool)])
    func reasonCodeNotice(code: String?, reason: String, text: String, signIn: Bool) {
        let account = UsageHUDAccount(id: "claude:work", name: "work", meters: [
            meter(window: nil, usedPct: nil, state: .failed, reason: reason, reasonCode: code, asOfSecondsAgo: nil),
        ])
        #expect(account.notice == UsageHUDNotice(text: text, needsSignIn: signIn))
    }

    @Test("the app knows each reason code of docs/usage-format.md and no other")
    func reasonCodesMatchTheSpec() throws {
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let spec = try String(contentsOf: repo.appendingPathComponent("docs/usage-format.md"), encoding: .utf8)
        let section = try #require(spec.components(separatedBy: "## Reason codes").dropFirst().first)
            .components(separatedBy: "\n## ")[0]
        let codes = section.split(separator: "\n").filter { $0.hasPrefix("| `") }
            .map { $0.dropFirst(3).prefix { $0 != "`" } }.map(String.init)
        #expect(codes == SwarmReasonCode.allCases.map(\.rawValue))
    }

    @Test("a Refresh during a poll read ends only after the refreshed meters are in the model")
    func refreshWaitsForTheQueuedRead() async throws {
        let command = UsageCommand([meter(usedPct: 10)])
        let held = HeldRead()
        let old = [meter(usedPct: 10)]
        let model = UsageHUDModel(
            loadUsage: { await held.holdFirst() ? old : try await command.read() },
            refresher: UsageRefresher(now: UsageTestClock(now).now) { try await command.refresh($0) },
            history: UsageHistoryStore(folder: nil),
            defaults: try #require(UserDefaults(suiteName: "UsageHUDModelTests.\(UUID().uuidString)")),
            now: UsageTestClock(now).now
        )
        let poll = Task { await model.read() }
        // Each wait has a bound and releases the read before its check, so a failure ends the test.
        var holding = false
        for _ in 0..<100_000 {
            if await held.isHolding {
                holding = true
                break
            }
            await Task.yield()
        }
        guard holding else {
            await held.release()
            Issue.record("the poll read did not start")
            return
        }
        await command.set([meter(usedPct: 50)])
        let refresh = Task { await model.refresh() }
        var refreshed = false
        for _ in 0..<100_000 {
            if await command.refreshed.count >= 2 {
                refreshed = true
                break
            }
            await Task.yield()
        }
        guard refreshed else {
            await held.release()
            Issue.record("the refresh did not call both providers")
            return
        }
        for _ in 0..<50 { await Task.yield() }
        // The poll read still holds the old answer, so the refresh is not over.
        #expect(model.isRefreshing)
        await held.release()
        await refresh.value
        #expect(!model.isRefreshing)
        #expect(model.meters.map(\.usedPct) == [50])
        await poll.value
    }

    // MARK: Reads

    @Test("a read fills the meters, the pressure rows and the read time")
    func read() async throws {
        let command = UsageCommand([meter(usedPct: 92)])
        let model = try makeModel(command)
        #expect(!model.hasLoaded && model.mood == .asleep)
        #expect(model.headerCaption == "Claude & Codex · No fresh usage")
        #expect(model.pill == nil)
        await model.read()
        #expect(model.hasLoaded && !model.readFailed)
        #expect(model.readAtSeconds == now)
        #expect(model.pressures.map(\.meterID) == ["claude:work:5h"])
        #expect(model.mood == .danger)
        #expect(model.headerCaption == "1 account · Ease off")
        #expect(model.pill?.usedPct == 92 && model.pill?.severity == .danger)
        #expect(model.footer(nowSeconds: now).text == "Usage from swarm")
    }

    @Test("a failed first read has no rows, and a later failed read keeps the last rows")
    func failedRead() async throws {
        let command = UsageCommand([meter()])
        await command.failRead(true)
        let model = try makeModel(command)
        await model.read()
        #expect(model.firstReadFailed && !model.hasLoaded && model.meters.isEmpty)
        await command.failRead(false)
        await model.read()
        #expect(!model.firstReadFailed && model.meters.count == 1)
        await command.failRead(true)
        await model.read()
        #expect(model.readFailed && !model.firstReadFailed && model.meters.count == 1)
        let footer = model.footer(nowSeconds: now)
        #expect(footer.text == "Swarm could not read usage. Click Refresh to try again." && footer.warning)
    }

    @Test("an older sample puts the clock note in the footer")
    func olderSampleFooter() async throws {
        let model = try makeModel(UsageCommand([meter(), meter(window: "7d", minutes: 10_080, state: .stale)]))
        await model.read()
        #expect(model.hasOlderSamples)
        let footer = model.footer(nowSeconds: now)
        #expect(footer.text == "Clock marks an older sample." && !footer.warning)
    }

    // MARK: Refresh

    @Test("Refresh asks Claude and Codex once, then reads the result")
    func refresh() async throws {
        let command = UsageCommand()
        let model = try makeModel(command)
        await command.set([meter()])
        await model.refresh()
        #expect(await command.refreshed.sorted() == ["claude", "codex"])
        #expect(model.meters.count == 1 && !model.isRefreshing)
        #expect(model.footer(nowSeconds: now).text == "Usage from swarm")
    }

    @Test("a second Refresh inside 300 seconds makes no call and shows when the next one is allowed")
    func refreshCooldown() async throws {
        let command = UsageCommand([meter()])
        let clock = UsageTestClock(now)
        let model = try makeModel(command, clock: clock)
        await model.refresh()
        clock.advance(by: 120)
        await command.set([meter(usedPct: 70)])
        await model.refresh()
        #expect(await command.refreshed.count == 2)
        // The cached values are still read.
        #expect(model.meters.first?.usedPct == 70)
        let footer = model.footer(nowSeconds: now + 120)
        #expect(footer.text == "Updated 2m ago. Next refresh in 3m." && !footer.warning)
        #expect(model.footer(nowSeconds: now + 300).text == "Usage from swarm")
    }

    @Test("a provider that Refresh could not reach is named in the footer")
    func refreshFailure() async throws {
        let command = UsageCommand([meter()])
        await command.failRefresh("codex", "codex is not on PATH")
        let model = try makeModel(command)
        await model.refresh()
        let footer = model.footer(nowSeconds: now)
        #expect(footer.text == "Last refresh: Codex: codex is not on PATH" && footer.warning)
        #expect(model.meters.count == 1)
    }

    @Test("a second Refresh inside 300 seconds keeps the failure text of the first one")
    func refreshFailureStaysInCooldown() async throws {
        let command = UsageCommand([meter()])
        await command.failRefresh("codex", "codex is not on PATH")
        let clock = UsageTestClock(now)
        let model = try makeModel(command, clock: clock)
        await model.refresh()
        clock.advance(by: 60)
        await model.refresh()
        #expect(await command.refreshed.count == 2)
        let footer = model.footer(nowSeconds: now + 60)
        #expect(footer.text == "Last refresh: Codex: codex is not on PATH" && footer.warning)
    }

    // MARK: Panel layout

    @Test("the content rect is the notch when collapsed and the card under the screen top when open")
    func contentRect() {
        let panel = CGSize(width: 640, height: 900)
        #expect(UsageHUDLayout.contentRect(isCollapsed: true, expandedHeight: 396, expandedWidth: 584,
                                           notchWidth: 126, notchHeight: 38, panelSize: panel)
            == CGRect(x: 257, y: 862, width: 126, height: 38))
        #expect(UsageHUDLayout.contentRect(isCollapsed: false, expandedHeight: 396, expandedWidth: 584,
                                           notchWidth: 126, notchHeight: 38, panelSize: panel)
            == CGRect(x: 28, y: 466, width: 584, height: 434))
        #expect(UsageHUDLayout.contentRect(isCollapsed: false, expandedHeight: 396, expandedWidth: 800, notchWidth: 126,
                                           notchHeight: 38, panelSize: CGSize(width: 1_000, height: 900))
            == CGRect(x: 100, y: 466, width: 800, height: 434))
    }

    @Test("the panel is centered at the top of its screen, and its height follows the screen")
    func placement() {
        let origin = UsageHUDLayout.notchAnchoredOrigin(
            size: CGSize(width: 380, height: 900), screenFrame: CGRect(x: 1_000, y: 200, width: 1_512, height: 982)
        )
        #expect(origin == CGPoint(x: 1_566, y: 282))
        #expect(UsageHUDLayout.panelHeight(screenHeight: 900) == 860)
        #expect(UsageHUDLayout.panelHeight(screenHeight: 300) == 400)
    }

    @Test("provider cards sit side by side only when each gets its minimum width")
    func cardWidths() {
        let minimums = [UsageHUDLayout.providerCardMinWidth(columns: 3), UsageHUDLayout.providerCardMinWidth(columns: 2)]
        #expect(minimums == [552, 426])
        #expect(UsageHUDLayout.providerCardWidths(minWidths: minimums, total: 991, gap: 14) == nil)
        #expect(UsageHUDLayout.providerCardWidths(minWidths: minimums, total: 992, gap: 14) == [552, 426])
        // Spare width splits in proportion, so the three-window card stays the wider one.
        #expect(UsageHUDLayout.providerCardWidths(minWidths: minimums, total: 1_304, gap: 14) == [728, 561])
        #expect(UsageHUDLayout.providerCardWidths(minWidths: [552], total: 2_000, gap: 14) == nil)
    }

    @Test("the card width has a default, a minimum, and a maximum of 90% of the screen")
    func widthClamp() {
        #expect(UsageHUDLayout.clampedPanelWidth(nil, screenWidth: 1_512) == 1_100)
        #expect(UsageHUDLayout.clampedPanelWidth(.nan, screenWidth: 1_512) == 1_100)
        #expect(UsageHUDLayout.clampedPanelWidth(nil, screenWidth: 1_000) == 900)
        #expect(UsageHUDLayout.clampedPanelWidth(500, screenWidth: 1_512) == 584)
        #expect(UsageHUDLayout.clampedPanelWidth(700, screenWidth: 1_512) == 700)
        #expect(UsageHUDLayout.clampedPanelWidth(5_000, screenWidth: 1_512) == 1_360)
        #expect(UsageHUDLayout.clampedPanelWidth(900, screenWidth: 800) == 720)
        // A screen too narrow for the maximum still keeps the minimum.
        #expect(UsageHUDLayout.clampedPanelWidth(900, screenWidth: 600) == 584)
    }

    @Test("the hover area covers the notch, its shoulders and the top edge of the screen")
    func hoverRects() {
        let notch = CGRect(x: 790, y: 1_131, width: 220, height: 38)
        let hover = UsageHUDLayout.fullNotchHoverRect(notch)
        #expect(hover == CGRect(x: 757, y: 1_126, width: 286, height: 44))
        #expect(hover.contains(CGPoint(x: notch.midX, y: notch.maxY)))
        #expect(hover.contains(CGPoint(x: notch.minX - 24, y: notch.midY)))
        #expect(hover.contains(CGPoint(x: notch.midX, y: notch.minY - 4)))
        // A pointer at the very top of the screen stays inside after the panel opens.
        let screenTop: CGFloat = 1_169
        let open = UsageHUDLayout.topEdgeInclusiveRect(CGRect(x: 600, y: 700, width: 600, height: screenTop - 700))
        #expect(open.contains(CGPoint(x: 900, y: screenTop)))
        #expect(UsageHUDLayout.physicalNotchRect(panelFrame: CGRect(x: 0, y: 0, width: 640, height: 900),
                                                 notchWidth: 220, notchHeight: 38)
            == CGRect(x: 210, y: 862, width: 220, height: 38))
    }

    // MARK: Panel state

    @Test("a new screen height while open waits for the next collapsed placement")
    func ceiling() throws {
        let model = try makeModel(UsageCommand())
        model.applyPanelCeiling(500)
        model.expandedContentHeight = 480
        model.isCollapsed = false
        model.applyPanelCeiling(900)
        #expect(model.panelSize.height == 900)
        // The open card keeps its measured height.
        #expect(model.expandedContentHeight == 480)
        model.isCollapsed = true
        model.applyPanelCeiling(900)
        #expect(model.expandedContentHeight == 900)
        model.expandedContentHeight = 700
        model.applyPanelCeiling(900)
        #expect(model.expandedContentHeight == 700)
    }

    @Test("a lower height limit from the bottom edge lowers the card height at once")
    func heightLimitLowersTheCard() throws {
        let model = try makeModel(UsageCommand())
        model.applyPanelCeiling(900)
        model.expandedContentHeight = 600
        model.resizePanelHeight(to: 400)
        #expect(model.expandedContentHeight == 400)
        // A higher limit gives height back, up to what the content needs.
        model.contentNeeds(height: 600)
        model.resizePanelHeight(to: 500)
        #expect(model.expandedContentHeight == 500)
        model.resizePanelHeight(to: 700)
        #expect(model.expandedContentHeight == 600)
        // The screen is the ceiling: 900 less the 38 pt notch.
        model.contentNeeds(height: 2_000)
        model.resizePanelHeight(to: 3_000)
        #expect(model.expandedContentHeight == 862)
    }

    @Test("the height limit has a floor and a new model reads the saved one")
    func heightLimit() throws {
        let suite = "UsageHUDModelTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let first = try makeModel(UsageCommand(), defaults: defaults)
        #expect(first.panelHeightLimit == nil)
        first.resizePanelHeight(to: 100)
        #expect(first.panelHeightLimit == UsageHUDLayout.minPanelHeightLimit)
        first.resizePanelHeight(to: 420)
        first.savePanelHeightLimit()
        #expect(try makeModel(UsageCommand(), defaults: defaults).panelHeightLimit == 420)
    }

    @Test("a saved width returns on a wide screen, and a small screen does not overwrite it")
    func savedWidth() throws {
        let suite = "UsageHUDModelTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let first = try makeModel(UsageCommand(), defaults: defaults)
        first.applyScreenWidth(1_512)
        #expect(first.panelWidth == 1_100)
        #expect(first.panelSize.width == 1_400)
        first.resizePanel(to: 760)
        first.savePanelWidth()
        let small = try makeModel(UsageCommand(), defaults: defaults)
        small.applyScreenWidth(800)
        #expect(small.panelWidth == 720)
        let wide = try makeModel(UsageCommand(), defaults: defaults)
        wide.applyScreenWidth(1_512)
        #expect(wide.panelWidth == 760)
    }

    // MARK: The prompt

    @Test("the prompt asks once per app run, only with no answer and no setup sheet on screen")
    func promptAsks() {
        #expect(UsageHUDPrompt.shouldAsk(answer: nil, askedThisRun: false, setupSheetOpen: false))
        #expect(!UsageHUDPrompt.shouldAsk(answer: nil, askedThisRun: false, setupSheetOpen: true))
        #expect(!UsageHUDPrompt.shouldAsk(answer: nil, askedThisRun: true, setupSheetOpen: false))
        #expect(!UsageHUDPrompt.shouldAsk(answer: true, askedThisRun: false, setupSheetOpen: false))
        #expect(!UsageHUDPrompt.shouldAsk(answer: false, askedThisRun: false, setupSheetOpen: false))
        #expect(UsageHUDPrompt.title == "Add the usage HUD?")
        #expect(UsageHUDPrompt.message == "Swarm can show Claude and Codex usage in a panel at the top of the screen. Hover over the notch to open it. You can turn it off in Settings › Accounts.")
        #expect([UsageHUDPrompt.acceptTitle, UsageHUDPrompt.declineTitle] == ["Add HUD", "No Thanks"])
    }

    @Test("Add HUD saves on, No Thanks saves off, and neither answer is asked again", arguments: [true, false])
    func promptAnswer(accepted: Bool) throws {
        let folder = try claimedChoicesFolder(
            FileManager.default.temporaryDirectory.appendingPathComponent("usage-hud-prompt-\(UUID().uuidString)")
        )
        defer { try? FileManager.default.removeItem(at: folder) }
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: folder))
        #expect(UsageHUDPrompt.shouldAsk(answer: selection.prefs.usageHUD, askedThisRun: false, setupSheetOpen: false))
        selection.setUsageHUD(accepted)
        // A new app run reads the saved answer.
        let nextRun = SettingsSelection(choices: OwnerChoicesStore(folder: folder))
        #expect(nextRun.prefs.usageHUD == accepted)
        #expect(!UsageHUDPrompt.shouldAsk(answer: nextRun.prefs.usageHUD, askedThisRun: false, setupSheetOpen: false))
    }

    @Test("at app start the saved answer alone decides, so the HUD needs no window")
    func shownAtLaunch() throws {
        let folder = try claimedChoicesFolder(
            FileManager.default.temporaryDirectory.appendingPathComponent("usage-hud-launch-\(UUID().uuidString)")
        )
        defer { try? FileManager.default.removeItem(at: folder) }
        #expect(!UsageHUDPrompt.showsAtLaunch(choices: OwnerChoicesStore(folder: folder)))
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: folder))
        selection.setUsageHUD(true)
        #expect(UsageHUDPrompt.showsAtLaunch(choices: OwnerChoicesStore(folder: folder)))
        selection.setUsageHUD(false)
        #expect(!UsageHUDPrompt.showsAtLaunch(choices: OwnerChoicesStore(folder: folder)))
        // No swarm home, or a choices file that cannot be read, shows no HUD.
        #expect(!UsageHUDPrompt.showsAtLaunch(choices: OwnerChoicesStore(folder: nil)))
        selection.setUsageHUD(true)
        try FileManager.default.removeItem(at: folder.appendingPathComponent("choices.lock"))
        try FileManager.default.createDirectory(at: folder.appendingPathComponent("choices.lock"), withIntermediateDirectories: false)
        #expect(!UsageHUDPrompt.showsAtLaunch(choices: OwnerChoicesStore(folder: folder)))
    }

    // MARK: The owner's answer

    @Test("the HUD answer is on, off or absent in choices.json")
    func answer() throws {
        let decoder = JSONDecoder()
        #expect(try decoder.decode(Prefs.self, from: Data("{}".utf8)).usageHUD == nil)
        #expect(try decoder.decode(Prefs.self, from: Data(#"{"usageHUD":"yes"}"#.utf8)).usageHUD == nil)
        #expect(try decoder.decode(Prefs.self, from: Data(#"{"usageHUD":false}"#.utf8)).usageHUD == false)
        #expect(!String(decoding: try JSONEncoder().encode(Prefs()), as: UTF8.self).contains("usageHUD"))

        let folder = try claimedChoicesFolder(
            FileManager.default.temporaryDirectory.appendingPathComponent("usage-hud-\(UUID().uuidString)")
        )
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        let selection = SettingsSelection(choices: choices)
        #expect(selection.prefs.usageHUD == nil)
        selection.setUsageHUD(true)
        #expect(selection.prefs.usageHUD == true && selection.saveError == nil)
        #expect(try choices.load().prefs.usageHUD == true)
        selection.setUsageHUD(false)
        #expect(SettingsSelection(choices: choices).prefs.usageHUD == false)
    }
}
