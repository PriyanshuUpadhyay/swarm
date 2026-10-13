import Foundation
import Testing
@testable import SwarmCore

@Suite("Skills checkout settings")
@MainActor
struct SkillsCheckoutSettingsTests {
    @Test("Saving displays the canonical path returned by the setting owner")
    func canonicalPath() async {
        let source = CheckoutSettingsFixture()
        source.result = "/canonical/swarm"
        let model = SkillsCheckoutSettingsModel(source: source)
        model.path = "/selected/swarm-link"
        await model.save()
        #expect(source.requests == ["/selected/swarm-link"])
        #expect(model.path == "/canonical/swarm")
        #expect(model.savedPath == "/canonical/swarm")
        #expect(model.error == nil)
    }

    @Test("Clear removes the setting and the field")
    func clearCheckout() async {
        let source = CheckoutSettingsFixture()
        source.checkoutPath = "/canonical/swarm"
        let model = SkillsCheckoutSettingsModel(source: source)
        #expect(model.canClear)
        await model.clear()
        #expect(source.requests.count == 1)
        #expect(source.requests.first == .some(nil))
        #expect(model.path.isEmpty)
        #expect(model.savedPath == nil)
        #expect(!model.canClear)
    }

    @Test("A failed save keeps the field and the prior canonical setting")
    func failureKeepsPath() async {
        let source = CheckoutSettingsFixture()
        source.checkoutPath = "/previous/swarm"
        source.failure = "The folder is not a swarm checkout."
        let model = SkillsCheckoutSettingsModel(source: source)
        model.path = "/invalid/folder"
        await model.save()
        #expect(model.path == "/invalid/folder")
        #expect(model.savedPath == "/previous/swarm")
        #expect(model.error == "The folder is not a swarm checkout.")
        #expect(!model.saving)
    }

    @Test("A failed settings read disables writes until the owner can load it")
    func failedLoad() async {
        let source = CheckoutSettingsFixture()
        source.checkoutPath = "/stale/swarm"
        source.loadError = "Could not read owner choices."
        let model = SkillsCheckoutSettingsModel(source: source)
        model.path = "/selected/swarm"
        #expect(!model.canSave)
        #expect(!model.canClear)
        await model.save()
        #expect(source.requests.isEmpty)
        #expect(model.error == "Could not read owner choices.")
        source.loadError = nil
        model.refresh()
        #expect(model.error == nil)
        #expect(model.canSave)
        #expect(model.path == "/selected/swarm")
    }
}

@MainActor
private final class CheckoutSettingsFixture: SkillsCheckoutSettingsSource {
    var checkoutPath: String?
    var loadError: String?
    var result: String?
    var failure: String?
    var requests: [String?] = []

    func setCheckout(_ path: String?) async throws -> String? {
        requests.append(path)
        if let failure { throw CheckoutSettingsFailure(message: failure) }
        checkoutPath = path == nil ? nil : result
        return checkoutPath
    }
}

private struct CheckoutSettingsFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}
