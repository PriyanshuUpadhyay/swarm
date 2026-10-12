import Foundation
import Synchronization
import Testing
@testable import SwarmCore

/// A clock that a test moves by hand. It also counts reads, so a test can see a call arrive.
final class UsageTestClock: Sendable {
    private let state: Mutex<(seconds: Int, reads: Int)>

    init(_ seconds: Int) { state = Mutex((seconds, 0)) }

    var reads: Int { state.withLock { $0.reads } }
    func advance(by seconds: Int) { state.withLock { $0.seconds += seconds } }
    func now() -> Int {
        state.withLock {
            $0.reads += 1
            return $0.seconds
        }
    }
}

/// Stands in for `swarm usage --refresh --provider <provider> --json`.
private actor RefreshCommand {
    private(set) var providers: [String] = []
    private var failure: SwarmProfileError?
    private var held: CheckedContinuation<Void, Never>?
    private var holds = false

    func hold() { holds = true }
    func fail(_ message: String?) { failure = message.map { .failed($0) } }
    func release() {
        holds = false
        held?.resume()
        held = nil
    }

    func run(_ provider: String) async throws -> SwarmUsage {
        providers.append(provider)
        if holds { await withCheckedContinuation { held = $0 } }
        if let failure { throw failure }
        return SwarmUsage(meters: [])
    }
}

@Suite("Usage refresher")
struct UsageRefresherTests {
    @Test("a provider is refreshed at most once per 300 seconds, and each provider has its own limit")
    func oncePerInterval() async throws {
        let clock = UsageTestClock(1_000)
        let command = RefreshCommand()
        let refresher = UsageRefresher(now: clock.now) { try await command.run($0) }
        #expect(try await refresher.refresh(provider: "claude") == .refreshed(SwarmUsage(meters: [])))
        clock.advance(by: 120)
        #expect(try await refresher.refresh(provider: "claude") == .cooldown(nextInSeconds: 180))
        #expect(try await refresher.refresh(provider: "codex") == .refreshed(SwarmUsage(meters: [])))
        clock.advance(by: 179)
        #expect(try await refresher.refresh(provider: "claude") == .cooldown(nextInSeconds: 1))
        clock.advance(by: 1)
        #expect(try await refresher.refresh(provider: "claude") == .refreshed(SwarmUsage(meters: [])))
        #expect(await command.providers == ["claude", "codex", "claude"])
    }

    @Test("a failed refresh counts, so a second click does not call the provider again")
    func failureCounts() async throws {
        let clock = UsageTestClock(1_000)
        let command = RefreshCommand()
        await command.fail("Sign in again")
        let refresher = UsageRefresher(now: clock.now) { try await command.run($0) }
        await #expect(throws: SwarmProfileError.failed("Sign in again")) { try await refresher.refresh(provider: "claude") }
        await command.fail(nil)
        #expect(try await refresher.refresh(provider: "claude") == .cooldown(nextInSeconds: 300))
        #expect(await command.providers == ["claude"])
    }

    @Test("two callers at the same time share one refresh")
    func sharedRun() async throws {
        let clock = UsageTestClock(1_000)
        let command = RefreshCommand()
        await command.hold()
        let refresher = UsageRefresher(now: clock.now) { try await command.run($0) }
        async let hud = refresher.refresh(provider: "claude")
        async let accountsPage = refresher.refresh(provider: "claude")
        // Each call reads the clock when it enters, so two reads mean both calls wait on the one run.
        while true {
            let started = await !command.providers.isEmpty
            if clock.reads >= 2 && started { break }
            await Task.yield()
        }
        await command.release()
        #expect(try await hud == .refreshed(SwarmUsage(meters: [])))
        #expect(try await accountsPage == .refreshed(SwarmUsage(meters: [])))
        #expect(await command.providers == ["claude"])
    }
}
