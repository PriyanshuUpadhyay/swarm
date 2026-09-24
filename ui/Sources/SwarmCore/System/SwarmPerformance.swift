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
