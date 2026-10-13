import Foundation
import Testing
@testable import SwarmCore

private let now = 1_000_000_000

private func sample(
    _ usedPct: Double, secondsAgo: Int = 0, account: String = "work", window: String = "5h"
) -> UsageHistorySample {
    UsageHistorySample(provider: "claude", account: account, window: window, usedPct: usedPct,
                       asOfSeconds: now - secondsAgo)
}

private func meter(
    window: String? = "5h", usedPct: Double? = 42, state: SwarmUsageState = .fresh, asOfSeconds: Int? = 480
) -> SwarmUsageMeter {
    SwarmUsageMeter(provider: "claude", account: "work", label: "work", window: window, windowMinutes: 300,
                    usedPct: usedPct, state: state, asOfSeconds: asOfSeconds)
}

@Suite("Usage history store")
struct UsageHistoryStoreTests {
    private func emptyFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("usage-history-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    @Test("prune drops samples older than seven days and keeps the one at the limit")
    func pruneAge() {
        let kept = UsageHistoryStore.prune(
            [sample(10, secondsAgo: 8 * 86_400), sample(5, secondsAgo: 7 * 86_400), sample(20, secondsAgo: 86_400)],
            nowSeconds: now
        )
        #expect(kept.map(\.usedPct) == [5, 20])
    }

    @Test("prune keeps the newest samples in time order when the count is over the cap")
    func pruneCount() {
        let many = (0..<10).map { sample(Double($0), secondsAgo: (10 - $0) * 3_600) }
        #expect(UsageHistoryStore.prune(many, nowSeconds: now, maxCount: 3).map(\.usedPct) == [7, 8, 9])
    }

    @Test("only a fresh meter with a window and a percent becomes a sample")
    func samplesFromMeters() {
        let meters = [
            meter(),
            meter(window: nil, usedPct: nil, state: .missing, asOfSeconds: nil),
            meter(window: "7d", usedPct: 30, state: .stale),
            meter(window: "fb", usedPct: nil, state: .failed),
        ]
        #expect(UsageHistoryStore.samples(from: meters, nowSeconds: 500) == [
            UsageHistorySample(provider: "claude", account: "work", window: "5h", usedPct: 42, asOfSeconds: 480),
        ])
        #expect(UsageHistoryStore.samples(from: [meter(asOfSeconds: nil)], nowSeconds: 500).first?.asOfSeconds == 500)
    }

    @Test("append saves to usage-history.json and a new store reads it")
    func roundTrip() throws {
        let folder = try emptyFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        UsageHistoryStore(folder: folder).append([sample(10)], nowSeconds: now)
        #expect(FileManager.default.fileExists(atPath: folder.appendingPathComponent("usage-history.json").path))
        #expect(UsageHistoryStore(folder: folder).load().map(\.usedPct) == [10])
    }

    @Test("a missing file, a missing folder and no folder give an empty history that still appends")
    func missingStorage() throws {
        let missing = FileManager.default.temporaryDirectory.appendingPathComponent("usage-history-\(UUID().uuidString)")
        for store in [UsageHistoryStore(folder: missing), UsageHistoryStore(folder: nil)] {
            #expect(store.load().isEmpty)
            #expect(store.append([sample(10)], nowSeconds: now).map(\.usedPct) == [10])
        }
        // The store never makes the swarm home; `swarm init` owns that folder.
        #expect(!FileManager.default.fileExists(atPath: missing.path))
    }

    @Test("a corrupt file reads as empty and the next append replaces it")
    func corruptFile() throws {
        let folder = try emptyFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        try Data("not json".utf8).write(to: folder.appendingPathComponent("usage-history.json"))
        let store = UsageHistoryStore(folder: folder)
        #expect(store.load().isEmpty)
        #expect(store.append([sample(5)], nowSeconds: now).map(\.usedPct) == [5])
        #expect(store.load().map(\.usedPct) == [5])
    }

    @Test("a sample with the same meter and time is saved once")
    func duplicates() throws {
        let folder = try emptyFolder()
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = UsageHistoryStore(folder: folder)
        store.append([sample(10)], nowSeconds: now)
        #expect(store.append([sample(10)], nowSeconds: now).count == 1)
        #expect(store.append([sample(10, secondsAgo: 60), sample(12)], nowSeconds: now).map(\.usedPct) == [10, 10])
    }

    @Test("different meters with the same time do not collide")
    func distinctMeters() {
        // Joined as text, "a:b" + "c" and "a" + "b:c" are the same string.
        let first = sample(1, account: "a:b", window: "c")
        let second = sample(2, account: "a", window: "b:c")
        #expect(UsageHistoryStore(folder: nil).append([first, second], nowSeconds: now).count == 2)
    }

    @Test("the byte cap drops the oldest samples and keeps the newest")
    func byteCap() throws {
        let many = (0..<50).map { sample(Double($0), secondsAgo: 50 - $0) }
        let capped = UsageHistoryStore.capToByteBudget(many, maxBytes: 600)
        #expect(try JSONEncoder().encode(capped).count <= 600)
        #expect(capped.count < many.count)
        #expect(capped.last?.usedPct == 49)
    }
}
