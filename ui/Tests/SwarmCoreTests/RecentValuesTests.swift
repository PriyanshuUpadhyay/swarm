import Testing
@testable import SwarmCore

@Suite("Recent values")
struct RecentValuesTests {
    @Test("The oldest key drops past the capacity; storing again makes a key newest")
    func capacity() {
        var recent = RecentValues<String, Int>(capacity: 4)
        for (index, key) in ["alpha", "beta", "gamma", "delta"].enumerated() { recent.set(index, for: key) }
        recent.set(10, for: "alpha")
        recent.set(4, for: "epsilon")
        #expect(recent["beta"] == nil)
        #expect(recent["alpha"] == 10)
        #expect(recent["gamma"] == 2)
        #expect(recent["epsilon"] == 4)
    }
}
