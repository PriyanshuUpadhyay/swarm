public struct SwarmUsageFailureState: Sendable {
    private var consecutiveFailures = 0

    public init() {}

    public mutating func succeeded() {
        consecutiveFailures = 0
    }

    public mutating func failed() -> Bool {
        consecutiveFailures += 1
        return consecutiveFailures >= 2
    }
}
