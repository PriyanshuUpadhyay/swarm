import Foundation
import OSLog

/// Opt-in timings for Console and Instruments. Stage names are fixed; no chat text or paths are logged.
public enum SwarmPerformance {
    private static let logger = Logger(
        subsystem: "io.github.priyanshuupadhyay.swarm", category: "performance"
    )
    private static let signposter = OSSignposter(logger: logger)

    public static var isEnabled: Bool {
        UserDefaults.standard.bool(forKey: "performanceLogging")
            || ProcessInfo.processInfo.environment["SWARM_PERF"] == "1"
    }

    public static func begin(_ name: StaticString) -> Span {
        guard isEnabled else { return Span(name: name, startedAt: 0, state: nil) }
        let state = signposter.beginInterval(name, id: signposter.makeSignpostID())
        return Span(name: name, startedAt: DispatchTime.now().uptimeNanoseconds, state: state)
    }

    public static func event(_ name: StaticString) {
        guard isEnabled else { return }
        signposter.emitEvent(name)
        logger.info("\(name.description, privacy: .public)")
    }

    public struct Span {
        private let name: StaticString
        private let startedAt: UInt64
        private let state: OSSignpostIntervalState?

        fileprivate init(name: StaticString, startedAt: UInt64, state: OSSignpostIntervalState?) {
            self.name = name
            self.startedAt = startedAt
            self.state = state
        }

        public func end(count: Int? = nil) {
            guard let state else { return }
            SwarmPerformance.signposter.endInterval(name, state)
            let elapsed = Double(DispatchTime.now().uptimeNanoseconds - startedAt) / 1_000_000
            if let count {
                SwarmPerformance.logger.info(
                    "\(name.description, privacy: .public) \(elapsed, privacy: .public) ms count=\(count)"
                )
            } else {
                SwarmPerformance.logger.info(
                    "\(name.description, privacy: .public) \(elapsed, privacy: .public) ms"
                )
            }
        }
    }
}

/// Measurement aid: `SWARM_OPEN_SCRIPT=N` makes the window open every workspace and chat in turn,
/// N rounds, and print how long each took to show content. Panes do not attach in this mode, so a
/// run against a real home never touches live agents.
public enum SwarmOpenScript {
    public static var rounds: Int {
        Int(ProcessInfo.processInfo.environment["SWARM_OPEN_SCRIPT"] ?? "") ?? 0
    }

    public static var isActive: Bool { rounds > 0 }

    /// p50, p95, and max of millisecond samples, nearest rank.
    public static func summary(_ samples: [Double]) -> (p50: Double, p95: Double, max: Double)? {
        guard !samples.isEmpty else { return nil }
        let sorted = samples.sorted()
        func rank(_ p: Double) -> Double { sorted[Swift.max(0, Int((p * Double(sorted.count)).rounded(.up)) - 1)] }
        return (rank(0.5), rank(0.95), sorted[sorted.count - 1])
    }
}
