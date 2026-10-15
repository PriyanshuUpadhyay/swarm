import Foundation
import Observation
import SwarmHUDCore

/// The HUD's data: the last good `swarm usage --json` read and the refresh state.
@MainActor @Observable
final class UsageModel {
    var accounts: [UsageAccount] = []
    /// When the last read succeeded; nil before the first one.
    var readAt: Date?
    /// Why the last read failed; nil after a good one. The last good accounts stay on screen.
    var readError: String?
    var refreshing: Set<String> = []
    /// Per provider, when the next manual refresh is allowed (ADR 0087: once in 300 s).
    var nextRefresh: [String: Date] = [:]
    var refreshError: String?

    var isOpen = false
    /// A fixed clock for `--snapshot`; nil uses the real one.
    var now: Date?
    var notch = CGSize(width: 200, height: Layout.minNotch)
    var phase: DayPhase = .at(Date())
    /// Set by the app delegate: opens the card for the keyboard or a VoiceOver action.
    @ObservationIgnored var toggleFromKeyboard: (() -> Void)?
    /// ⌃⌥⌘U was registered; another app may hold it.
    var hotKeyReady = false
    /// A theme the owner pinned from the menu; nil follows the clock.
    var pinnedPhase: DayPhase? {
        didSet { UserDefaults.standard.set(pinnedPhase?.rawValue, forKey: "pinnedPhase"); updatePhase() }
    }

    private let swarm: String?
    private var reading = false
    private var readQueued = false

    init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        swarm = swarmPath(environment: environment)
        pinnedPhase = UserDefaults.standard.string(forKey: "pinnedPhase").flatMap(DayPhase.init(rawValue:))
    }

    func updatePhase(now: Date = Date()) {
        phase = pinnedPhase ?? .at(now)
    }

    /// The cached read: no provider call, so it may run as often as the HUD likes.
    func read() {
        guard !reading else { readQueued = true; return }
        reading = true
        let swarm = swarm
        Task {
            let result = await offPool { run(swarm, ["usage", "--json"], timeout: 8) }
            apply(result)
            reading = false
            if readQueued { readQueued = false; read() }
        }
    }

    private func apply(_ result: Result<Data, RunError>) {
        let snapshot: UsageSnapshot
        switch result {
        case .failure(let error):
            readError = error.text
            return
        case .success(let data):
            guard let decoded = try? UsageSnapshot.decode(data) else {
                readError = "swarm usage --json gave no swarm.usage.v1 reply"
                return
            }
            snapshot = decoded
        }
        accounts = SwarmHUDCore.accounts(from: snapshot.meters)
        readAt = Date()
        readError = nil
    }

    /// A provider call per provider with a meter, never automatic, at most once in 300 s each.
    func refresh(now: Date = Date()) {
        let providers = Set(accounts.map(\.provider)).intersection(["claude", "codex"])
            .filter { !refreshing.contains($0) && (nextRefresh[$0] ?? .distantPast) <= now }
        guard !providers.isEmpty else { return }
        refreshError = nil
        let swarm = swarm
        for provider in providers {
            refreshing.insert(provider)
            nextRefresh[provider] = now.addingTimeInterval(300)
            Task {
                let result = await offPool {
                    run(swarm, ["usage", "--refresh", "--provider", provider, "--json"], timeout: 30)
                }
                if case .failure(let error) = result { refreshError = "\(provider.capitalized): \(error.text)" }
                refreshing.remove(provider)
                read()
            }
        }
    }

    var canRefresh: Bool {
        let now = Date()
        return accounts.contains { ["claude", "codex"].contains($0.provider) && (nextRefresh[$0.provider] ?? .distantPast) <= now }
            && refreshing.isEmpty
    }
}

/// `SWARM_BIN` (set by the LaunchAgent) else the usual install places, because launchd's PATH is bare.
func swarmPath(environment: [String: String]) -> String? {
    if let path = environment["SWARM_BIN"], !path.isEmpty { return path }
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    return ["/opt/homebrew/bin/swarm", "/usr/local/bin/swarm", "\(home)/.cargo/bin/swarm"]
        .first { FileManager.default.isExecutableFile(atPath: $0) }
}

struct RunError: Error {
    let text: String
}

/// `run` blocks up to its deadline, so it waits on a GCD thread, not on the Swift concurrency pool.
nonisolated func offPool<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
    await withCheckedContinuation { done in DispatchQueue.global().async { done.resume(returning: work()) } }
}

/// Runs `swarm` with a deadline and returns its stdout. stderr names the failure.
nonisolated func run(_ executable: String?, _ arguments: [String], timeout: TimeInterval) -> Result<Data, RunError> {
    guard let executable else { return .failure(RunError(text: "swarm not found; set SWARM_BIN")) }
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    let out = Pipe(), err = Pipe()
    process.standardOutput = out
    process.standardError = err
    process.standardInput = FileHandle.nullDevice
    let exited = DispatchSemaphore(value: 0)
    process.terminationHandler = { _ in exited.signal() }
    do { try process.run() } catch { return .failure(RunError(text: "could not start swarm")) }
    // Drain both pipes while the child runs, so neither a long meter list nor a long error fills one.
    nonisolated(unsafe) var data = Data(), errorData = Data()
    let reads = DispatchGroup()
    DispatchQueue.global().async(group: reads) { data = out.fileHandleForReading.readDataToEndOfFile() }
    DispatchQueue.global().async(group: reads) { errorData = err.fileHandleForReading.readDataToEndOfFile() }
    // One deadline for both waits: a child can close its pipes and still run.
    let deadline = DispatchTime.now() + timeout
    if reads.wait(timeout: deadline) == .timedOut || exited.wait(timeout: deadline) == .timedOut {
        process.terminate()
        // A provider helper that swarm started can keep a pipe open after swarm exits, so the reads
        // may never end; give up on them (two parked threads) rather than on every later read.
        _ = reads.wait(timeout: .now() + 2)
        return .failure(RunError(text: "timed out"))
    }
    guard process.terminationStatus == 0 else {
        let line = String(decoding: errorData, as: UTF8.self).split(separator: "\n").last.map(String.init)
        return .failure(RunError(text: line ?? "swarm failed"))
    }
    return .success(data)
}
