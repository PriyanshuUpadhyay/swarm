// SwarmNotify: posts swarm's owner notices and focuses a pane on a click (ADR 0077).
// It has no window and no Dock icon (LSUIElement). `swarm` starts it with `open -g` and the env
// SWARM_HOME and SWARM_BIN; macOS starts it with no env for a click on an old notice.
// It drains at launch and at each reopen, never polls, and quits after `idleSeconds` with no work.
import AppKit
import OSLog
import UserNotifications

/// What a posted notice needs to focus its pane later, in any helper process.
struct Target: Sendable {
    let swarm: String
    let home: String?
}

/// One row of `swarm notices --due`.
struct DueNotice: Decodable, Sendable {
    let sessionId: String?
    let agentId: String?
    let title: String
    let body: String
    let sound: Bool
}

/// Runs `swarm` with SWARM_HOME set to `home` when one is known. Returns its exit status, stdout
/// and stderr. It runs off the main thread.
func runSwarm(_ target: Target, _ arguments: [String]) async -> (Int32, Data, String) {
    await withCheckedContinuation { continuation in
        DispatchQueue.global().async {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: target.swarm)
            process.arguments = arguments
            var environment = ProcessInfo.processInfo.environment
            if let home = target.home { environment["SWARM_HOME"] = home }
            process.environment = environment
            let out = Pipe(), err = Pipe()
            process.standardOutput = out
            process.standardError = err
            process.standardInput = FileHandle.nullDevice
            do { try process.run() } catch {
                continuation.resume(returning: (-1, Data(), "swarm: \(error.localizedDescription)"))
                return
            }
            let data = out.fileHandleForReading.readDataToEndOfFile()
            let text = String(decoding: err.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            process.waitUntilExit()
            continuation.resume(returning: (process.terminationStatus, data, text))
        }
    }
}

@MainActor
final class Helper: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    nonisolated static let category = "swarm.chat"
    static let idleSeconds: TimeInterval = 20
    let center = UNUserNotificationCenter.current()
    var busy = 0
    var draining = false
    var again = false
    var idle: Timer?

    /// The env from `swarm`, else `<prefix>/bin/swarm` for this bundle at `<prefix>/libexec` (ADR 0079).
    var target: Target {
        let env = ProcessInfo.processInfo.environment
        let bundled = Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("bin/swarm").path
        return Target(swarm: env["SWARM_BIN"].flatMap { $0.isEmpty ? nil : $0 } ?? bundled,
                      home: env["SWARM_HOME"].flatMap { $0.isEmpty ? nil : $0 })
    }

    func applicationWillFinishLaunching(_ notification: Notification) {
        // A click that starts this process is delivered only to a delegate set before launch ends.
        center.delegate = self
        center.setNotificationCategories([UNNotificationCategory(identifier: Self.category, actions: [],
                                                                 intentIdentifiers: [], options: [])])
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        log("launch pid \(ProcessInfo.processInfo.processIdentifier)")
        drain()
    }

    /// `open -g` on a running helper lands here: a new row is waiting.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        drain()
        return false
    }

    func drain() {
        // A reopen during a drain runs one more drain, so a row added meanwhile is not left behind.
        if draining { again = true; return }
        draining = true
        begin()
        Task {
            repeat {
                again = false
                await drainOnce()
            } while again
            draining = false
            end()
        }
    }

    func drainOnce() async {
        let target = target
        let (status, data, error) = await runSwarm(target, ["notices", "--due"])
        guard status == 0 else { log("notices --due failed: \(error)"); return }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let rows = try? decoder.decode([DueNotice].self, from: data), !rows.isEmpty else { return }
        do {
            guard try await center.requestAuthorization(options: noticeOptions) else {
                log("notice permission denied; \(rows.count) notice(s) not shown")
                return
            }
        } catch {
            log("notice permission failed: \(error); \(rows.count) notice(s) not shown")
            return
        }
        for row in rows {
            let content = UNMutableNotificationContent()
            content.title = row.title
            content.body = row.body
            content.categoryIdentifier = Self.category
            content.sound = row.sound ? .default : nil
            var info: [String: String] = ["swarm": target.swarm]
            if let home = target.home { info["home"] = home }
            if let session = row.sessionId { info["sessionID"] = session }
            if let agent = row.agentId { info["agentID"] = agent }
            content.userInfo = info
            do {
                try await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
                log("posted \(row.title): \(row.body)")
            } catch {
                log("post failed: \(error.localizedDescription)")
            }
        }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            didReceive response: UNNotificationResponse) async {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier,
              response.notification.request.content.categoryIdentifier == Self.category else { return }
        let info = response.notification.request.content.userInfo
        let session = info["sessionID"] as? String
        let agent = info["agentID"] as? String ?? "orchestrator"
        await MainActor.run { begin() }
        // A distinct name: older Swift reads `target` in the closure as the local declared below.
        let fallback = await MainActor.run { self.target }
        // An upgrade's cleanup can remove the swarm that an old notice names, so that notice uses
        // this helper's swarm with its own home.
        let stored = (info["swarm"] as? String).flatMap { FileManager.default.isExecutableFile(atPath: $0) ? $0 : nil }
        let clickTarget = Target(swarm: stored ?? fallback.swarm, home: info["home"] as? String)
        // The same rule as the app's NoticeClick (ADR 0069): a failure posts one error notice
        // with no click action, so it cannot repeat the failed focus.
        var failure: String?
        if let session {
            let (status, _, error) = await runSwarm(clickTarget, ["focus", session, agent])
            if status != 0 { failure = error.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines) }
            await MainActor.run { log("focus \(session) \(agent) exit \(status)") }
        } else {
            failure = "swarm: pane closed"
        }
        if let failure {
            let content = UNMutableNotificationContent()
            content.title = "Swarm"
            content.body = failure.isEmpty ? "swarm: focus failed" : failure
            try? await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }
        await MainActor.run { end() }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }

    func begin() {
        busy += 1
        idle?.invalidate()
        idle = nil
    }

    /// Quits `idleSeconds` after the last drain or click ends.
    func end() {
        busy -= 1
        guard busy == 0 else { return }
        idle = Timer.scheduledTimer(withTimeInterval: Self.idleSeconds, repeats: false) { _ in
            MainActor.assumeIsolated {
                log("idle exit")
                NSApp.terminate(nil)
            }
        }
    }
}

/// One line to stderr and the unified log, because a helper that `open` starts has no terminal.
func log(_ line: String) {
    FileHandle.standardError.write(Data("SwarmNotify: \(line)\n".utf8))
    logger.notice("\(line, privacy: .public)")
}

let logger = Logger(subsystem: "io.github.priyanshuupadhyay.swarm.notify", category: "notice")

/// One option set for posting and `--authorize`, because macOS keeps the first set it grants.
let noticeOptions: UNAuthorizationOptions = [.alert, .sound]

/// `--status` prints the owner's notice settings for this helper as one JSON line, and
/// `--authorize` asks for permission first. `swarm notices` runs the executable directly for them,
/// so the rule that reads the answer stays in Rust.
func printSettings() {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
        let authorization = switch settings.authorizationStatus {
        case .notDetermined: "notDetermined"
        case .denied: "denied"
        case .authorized: "authorized"
        case .provisional: "provisional"
        case .ephemeral: "ephemeral"
        @unknown default: "unknown"
        }
        let style = switch settings.alertStyle {
        case .none: "none"
        case .banner: "banner"
        case .alert: "alert"
        @unknown default: "unknown"
        }
        print("{\"authorization\":\"\(authorization)\",\"style\":\"\(style)\"}")
        exit(0)
    }
}

switch CommandLine.arguments.dropFirst().first {
case "--status":
    printSettings()
    dispatchMain()
case "--authorize":
    // The prompt is asked from an accessory app process, as the posting path asks it.
    NSApplication.shared.setActivationPolicy(.accessory)
    UNUserNotificationCenter.current().requestAuthorization(options: noticeOptions) { _, _ in printSettings() }
    NSApplication.shared.run()
default:
    break
}

let helper = Helper()
let application = NSApplication.shared
application.delegate = helper
application.setActivationPolicy(.accessory)
application.run()
