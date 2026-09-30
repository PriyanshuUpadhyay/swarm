import Synchronization
import Testing
@testable import SwarmCore

@Suite("Profile catalog")
struct SwarmProfileCatalogTests {
    @Test("a failed provider read is read again, and a good one serves every later caller")
    func providerReadRetriesThenStays() async throws {
        let reads = Mutex(0)
        let catalog = SwarmProfileCatalog(
            profiles: { SwarmProfileList(revision: "r1", minUsageLeftPct: 5, imported: nil, profiles: []) },
            providers: {
                let count = reads.withLock { $0 += 1; return $0 }
                if count == 1 { throw SwarmProfileError.failed("swarm not found") }
                return []
            }
        )

        await #expect(throws: SwarmProfileError.failed("swarm not found")) {
            try await catalog.providers()
        }
        #expect(try await catalog.providers() == [])
        #expect(try await catalog.providers() == [])
        #expect(reads.withLock { $0 } == 2)
    }

    @Test("each profile read goes to swarm again and keeps the last list")
    func profileReadIsFresh() async throws {
        let reads = Mutex(0)
        let catalog = SwarmProfileCatalog(
            profiles: {
                reads.withLock { $0 += 1 }
                return SwarmProfileList(revision: "r1", minUsageLeftPct: 5, imported: nil, profiles: [])
            },
            providers: { [] }
        )

        #expect(await catalog.cachedProfiles == nil)
        _ = try await catalog.profiles()
        _ = try await catalog.profiles()
        #expect(reads.withLock { $0 } == 2)
        #expect(await catalog.cachedProfiles != nil)
    }
}
