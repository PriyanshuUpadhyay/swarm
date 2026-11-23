import Foundation
import Observation
import SwarmHUDCore

/// The HUD's data: the last good `swarm usage --json` read, its pace history, and refresh state.
@MainActor @Observable
final class UsageModel {
    var accounts: [UsageAccount] = [] { didSet { updateWings() } }
    var history: [HistorySample] = [] { didSet { updateWings() } }
    /// Working and waiting swarm agents per provider, from `swarm working --json`; empty when none or unknown.
    var agents = WorkingSnapshot() { didSet { updateWings() } }
    /// ADR 0147: the waiting agents and the unseen chat results, from `swarm notices --inbox --json`.
    var inbox = InboxSnapshot() { didSet { updateWings() } }
    /// The card under the notch for what just came in; nil while the card is open.
    var popup: Popup? {
        didSet {
            // The cursor that held the last popup does not hold the next one.
            if popup == nil { popupHovered = false }
            if popup != oldValue { wingsChanged?() }
        }
    }
    /// Set by the app delegate: says a new popup line aloud for VoiceOver.
    @ObservationIgnored var announce: ((String) -> Void)?
    /// Set by the app delegate: plays the sound for a chat that converged (ADR 0156).
    @ObservationIgnored var chime: (() -> Void)?
    /// The cursor is on the popup, so a done popup waits for it to leave.
    @ObservationIgnored var popupHovered = false
    /// The keys of the last inbox read; nil before the first, so a new HUD pops nothing old.
    @ObservationIgnored private var knownKeys: Set<String>?
    /// Why the last Open, answer, or Clear failed, such as "the question changed; read it again".
    /// Each failure is said aloud, also one with the same text as the last.
    var actionError: String? {
        didSet { if let actionError { announce?(actionError) } }
    }
    /// The chats with an Open on the way, so a double click runs one `swarm focus`.
    @ObservationIgnored var opening: Set<String> = []
    /// The need cards with an answer on the way, so a second click cannot send a second answer.
    var answering: Set<String> = []
    /// The closed wings' words and how much they widen each wing, kept with their inputs, because
    /// the hover check reads the closed size on every mouse move.
    private(set) var wings = WingText()
    private(set) var wingExtra: CGFloat = 0
    /// Set by the app delegate: the closed shape changed width under a cursor that may not move.
    @ObservationIgnored var wingsChanged: (() -> Void)?
    /// When an agent last worked or waited; the mascot sleeps 5 minutes later. Launch counts, so a
    /// new HUD starts awake. Unobserved: the 30 s clock re-reads it, and each read would redraw.
    @ObservationIgnored var lastActive = Date()
    /// The provider whose agents worked last, so an idle mascot keeps its family.
    var lastWorking: String?
    /// When the last read succeeded; nil before the first one.
    var readAt: Date?
    /// Why the last read failed; nil after a good one. The last good accounts stay on screen.
    var readError: String?
    var refreshing: Set<String> = []
    var renewing: Set<String> = []
    /// Per provider, when the next manual refresh is allowed (ADR 0087: once in 300 s).
    var nextRefresh: [String: Date] = [:]
    var refreshError: String?

    var isOpen = false
    /// A fixed clock for `--snapshot`; nil uses the real one.
    var now: Date? { didSet { updateWings() } }
    /// A fixed mascot turn for `--snapshot`; nil takes turns every 10 minutes.
    var fixedMascot: Int?
    var notch = CGSize(width: 200, height: Layout.minNotch)
    var phase: DayPhase = .at(Date())
    /// Set by the app delegate: opens the card for the keyboard or a VoiceOver action.
    @ObservationIgnored var toggleFromKeyboard: (() -> Void)?
    /// Set by the app delegate: closes the card and the popup at once, after a click.
    @ObservationIgnored var closeHUD: (() -> Void)?
    /// Set by the app delegate: Swarm.app is the front app, so the owner sees new items there.
    @ObservationIgnored var swarmInFront: (() -> Bool)?
    /// ⌃⌥⇧⌘U was registered; another app may hold it.
    var hotKeyReady = false
    /// A theme the owner pinned from the menu; nil follows the clock.
    var pinnedPhase: DayPhase? {
        didSet { UserDefaults.standard.set(pinnedPhase?.rawValue, forKey: "pinnedPhase"); updatePhase() }
    }

    let swarm: String?
    private let historyURL: URL
    private var reading = false
    private var readQueued = false
    private var readingWorking = false
    private var readingInbox = false
    /// A change signal that came during a read; the read runs again after it, so the change is not
    /// left for the slow poll.
    private var workingQueued = false
    private var inboxQueued = false
    /// Why the last inbox read failed; nil after a good one. The last good inbox stays.
    var inboxError: String? {
        didSet { if let inboxError, inboxError != oldValue { announce?(inboxError) } }
    }
    /// The card's one error line: a failed action first, because the owner just did it.
    var shownError: String? { actionError ?? inboxError }
    /// Goes up when an Open, answer, or Clear starts and when it ends. A read that saw it change
    /// may hold rows from before the action, so it is dropped and read again. The poll never
    /// changes it, so a slow read still lands.
    @ObservationIgnored private var inboxEpoch = 0
    private var refreshQueued: Set<String> = []

    init(environment: [String: String] = ProcessInfo.processInfo.environment, historyURL: URL? = nil) {
        swarm = swarmPath(environment: environment)
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        self.historyURL = historyURL ?? support.appending(path: "SwarmHUD/history.json")
        pinnedPhase = UserDefaults.standard.string(forKey: "pinnedPhase").flatMap(DayPhase.init(rawValue:))
        // A missing or broken file only costs the pace until new samples come in.
        history = (try? JSONDecoder().decode([HistorySample].self, from: Data(contentsOf: self.historyURL))) ?? []
    }

    private func updateWings() {
        let text = wingText(agents, best: bestAccount(accounts, history: history, now: now ?? Date()), inbox: inbox)
        guard text != wings else { return }
        (wings, wingExtra) = (text, Layout.wingExtra(text))
        wingsChanged?()
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
        let now = Date()
        accounts = SwarmHUDCore.accounts(from: snapshot.meters)
        readAt = now
        readError = nil
        let updated = appendHistory(history, samples(from: accounts), now: now)
        if updated != history {
            history = updated
            saveHistory()
        }
    }

    private func saveHistory() {
        try? FileManager.default.createDirectory(at: historyURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? JSONEncoder().encode(history).write(to: historyURL, options: .atomic)
    }

    /// Reads the store only, so it may run every few seconds; a failure reads as nothing working.
    func readWorking() {
        guard !readingWorking else { workingQueued = true; return }
        readingWorking = true
        let swarm = swarm
        Task {
            let result = await offPool { run(swarm, ["working", "--json"], timeout: 2) }
            let agents = (try? result.get()).map(WorkingSnapshot.read) ?? WorkingSnapshot()
            if agents.isActive { lastActive = Date() }
            if let busy = busiest(agents.working), busy != lastWorking { lastWorking = busy }
            if agents != self.agents { self.agents = agents }
            readingWorking = false
            if workingQueued { workingQueued = false; readWorking() }
        }
    }

    /// The store and at most three screen reads; a failure keeps the last inbox on screen.
    func readInbox() {
        guard !readingInbox else { inboxQueued = true; return }
        readingInbox = true
        let swarm = swarm, epoch = inboxEpoch
        Task {
            let result = await offPool { run(swarm, ["notices", "--inbox", "--json"], timeout: 3) }
            readingInbox = false
            guard epoch == inboxEpoch else { inboxQueued = false; readInbox(); return }
            defer { if inboxQueued { inboxQueued = false; readInbox() } }

            // A failed read keeps the last inbox on screen, and the error line says it is old.
            switch result {
            case .success(let data):
                if let inbox = InboxSnapshot.read(data) {
                    inboxError = nil
                    apply(inbox: inbox, now: Date())
                } else {
                    inboxError = "Could not read the inbox: this swarm prints another format"
                }
            case .failure(let error):
                inboxError = "Could not read the inbox: \(error.text)"
            }
        }
    }

    /// An Open, answer, or Clear starts to change the store.
    func inboxChanging() { inboxEpoch += 1 }

    /// The action is done, so the inbox is read again.
    func inboxChanged() {
        inboxEpoch += 1
        readInbox()
    }

    func apply(inbox: InboxSnapshot, now: Date) {
        let new = newPopupItems(inbox, known: knownKeys)
        knownKeys = inboxKeys(inbox)
        // One sound per read, so a burst of chats that end together plays once.
        if playsConvergedSound(inbox, new: new) { chime?() }
        if inbox != self.inbox { self.inbox = inbox }

        var next = prunePopup(popup, inbox: inbox)
        // The open card lists the same items, so it needs no popup. Swarm.app in front shows them too.
        if !isOpen, swarmInFront?() != true { next = mergePopup(next, new: new, now: now) }
        popup = next

        // The open card shows a new item at once, so VoiceOver hears it there too.
        if let first = new.first, popup != nil || isOpen {
            announce?("\(first.title) \(first.word): \(first.body)")
        }
        if !new.isEmpty, let shown = popup, !shown.stays {
            Task {
                try? await Task.sleep(for: .seconds(Popup.peek))
                closeExpiredPopup()
            }
        }
    }

    /// A done popup closes once its peek ends and the cursor is off it.
    func closeExpiredPopup(now: Date = Date()) {
        if popup?.expired(now: now, hovered: popupHovered) == true { popup = nil }
    }

    /// A provider call per provider with a meter, never automatic, at most once in 300 s each.
    /// An expired account is renewed first; its renewal fetches its provider, so that one is skipped here.
    func refresh(now: Date = Date()) {
        accounts.filter { $0.renewalArguments != nil }.forEach(renew)
        let providers = Set(accounts.map(\.provider)).intersection(["claude", "codex"])
            .subtracting(accounts.filter { renewing.contains($0.id) }.map(\.provider))
            .filter { !refreshing.contains($0) && (nextRefresh[$0] ?? .distantPast) <= now }
        guard !providers.isEmpty else { return }
        refreshError = nil
        for provider in providers {
            refresh(provider: provider, now: now)
        }
    }

    private func refresh(provider: String, now: Date = Date()) {
        guard refreshing.insert(provider).inserted else { refreshQueued.insert(provider); return }
        nextRefresh[provider] = now.addingTimeInterval(300)
        let swarm = swarm
        Task {
            let result = await offPool {
                run(swarm, ["usage", "--refresh", "--provider", provider, "--json"], timeout: 30)
            }
            if case .failure(let error) = result { refreshError = "\(provider.capitalized): \(error.text)" }
            refreshing.remove(provider)
            read()
            if refreshQueued.remove(provider) != nil { refresh(provider: provider) }
        }
    }

    func renew(_ account: UsageAccount) {
        guard let arguments = account.renewalArguments, renewing.insert(account.id).inserted else { return }
        let swarm = swarm
        Task {
            await offPool { runRenewal(swarm, arguments) }
            renewing.remove(account.id)
            // A renewal must fetch again even during the manual-refresh cooldown. If a fetch is
            // already running, queue one after it so its pre-renewal result cannot hide success.
            refreshError = nil
            refresh(provider: account.provider)
        }
    }

    var canRefresh: Bool {
        let now = Date()
        let renewable = accounts.contains { $0.renewalArguments != nil && !renewing.contains($0.id) }
        return (renewable || accounts.contains { ["claude", "codex"].contains($0.provider) && (nextRefresh[$0.provider] ?? .distantPast) <= now })
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

/// Runs only the token renewal; usage is fetched by the model after this returns.
nonisolated func runRenewal(_ executable: String?, _ arguments: [String], timeout: TimeInterval = 60) {
    guard let executable else { return }
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    process.currentDirectoryURL = FileManager.default.homeDirectoryForCurrentUser
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    let exited = DispatchSemaphore(value: 0)
    process.terminationHandler = { _ in exited.signal() }
    do { try process.run() } catch { return }
    guard exited.wait(timeout: .now() + timeout) == .timedOut, process.isRunning else { return }
    process.terminate()
    // A child that ignores SIGTERM must not outlive the spinner, or a second Renew starts a second run.
    if exited.wait(timeout: .now() + 5) == .timedOut, process.isRunning { kill(process.processIdentifier, SIGKILL) }
}

/// Runs `swarm` with a deadline and returns its stdout. stderr names the failure.
nonisolated func run(_ executable: String?, _ arguments: [String], timeout: TimeInterval,
                     environment: [String: String] = [:]) -> Result<Data, RunError> {
    guard let executable else { return .failure(RunError(text: "swarm not found; set SWARM_BIN")) }
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    if !environment.isEmpty {
        process.environment = ProcessInfo.processInfo.environment.merging(environment) { $1 }
    }
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
