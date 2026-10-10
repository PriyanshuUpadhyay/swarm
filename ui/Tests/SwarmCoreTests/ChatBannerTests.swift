import Testing
@testable import SwarmCore

@Suite("Chat banner")
struct ChatBannerTests {
    @Test("Action, refresh and model errors appear in order and Dismiss clears only the shown slot")
    func priorityAndDismissal() {
        var banner = ChatBanner()
        banner.setModel("Model failed.")
        banner.refreshFailed("Refresh failed.", token: 1)
        banner.setAction("Close failed.")
        #expect(banner.visible?.source == .action && banner.visible?.text == "Close failed.")
        banner.dismiss()
        #expect(banner.visible?.source == .refresh && banner.visible?.text == "Refresh failed.")
        #expect(banner.announcement == "Refresh failed.")
        banner.dismiss()
        #expect(banner.visible?.source == .model && banner.visible?.text == "Model failed.")
        #expect(banner.announcement == "Model failed.")
        banner.dismiss()
        #expect(banner.visible == nil && banner.announcement == nil)
    }

    @Test("Hidden failures stay silent and both lower slots announce once when revealed")
    func hiddenAnnouncements() {
        var banner = ChatBanner()
        banner.setAction("Action failed.")
        banner.refreshFailed("Refresh failed.", token: 1)
        banner.setModel("Model failed.")
        #expect(banner.announcement == nil)
        banner.setAction(nil)
        #expect(banner.announcement == "Refresh failed.")
        banner.setAction(nil)
        #expect(banner.announcement == nil)
        banner.refreshSucceeded(token: 2)
        #expect(banner.announcement == "Model failed.")
        banner.refreshSucceeded(token: 2)
        #expect(banner.announcement == nil)
    }

    @Test("A model failure also announces when an action error clears directly")
    func revealModelAfterAction() {
        var banner = ChatBanner()
        banner.setAction("Action failed.")
        banner.setModel("Model failed.")
        banner.dismiss()
        #expect(banner.visible?.source == .model && banner.announcement == "Model failed.")
    }

    @Test("Repeated action and refresh failures announce, and clearing the last slot stays silent")
    func repeatedFailures() {
        var banner = ChatBanner()
        banner.setAction("Action failed.")
        banner.setAction("Action failed.")
        #expect(banner.announcement == "Action failed.")
        banner.dismiss()
        #expect(banner.announcement == nil)
        banner.refreshFailed("Refresh failed.", token: 1)
        banner.refreshFailed("Refresh failed.", token: 2)
        #expect(banner.announcement == "Refresh failed.")
        banner.dismiss()
        #expect(banner.announcement == nil)
    }

    @Test("A dismissed model error stays hidden until its text changes or it recovers")
    func dismissedModel() {
        var banner = ChatBanner()
        banner.setModel("Model failed.")
        banner.dismiss()
        banner.setModel("Model failed.")
        #expect(banner.visible == nil)
        banner.setModel("Model failed again.")
        #expect(banner.announcement == "Model failed again.")
        banner.dismiss()
        banner.setModel(nil)
        banner.setModel("Model failed again.")
        #expect(banner.announcement == "Model failed again.")
    }

    @Test("A newer success rejects an older refresh failure and keeps a standing action error")
    func overtakenFailure() {
        var banner = ChatBanner()
        banner.setAction("Action failed.")
        banner.refreshFailed("Refresh failed.", token: 1)
        banner.refreshSucceeded(token: 3)
        banner.refreshFailed("Old refresh failed.", token: 2)
        #expect(banner.visible?.source == .action && banner.announcement == nil)
        banner.dismiss()
        #expect(banner.visible == nil)
        banner.refreshFailed("New refresh failed.", token: 4)
        #expect(banner.visible?.text == "New refresh failed." && banner.announcement == "New refresh failed.")
    }

    @Test("A model recovery before the refresh clear does not reveal the old model failure")
    func combinedRecovery() {
        var banner = ChatBanner()
        banner.setModel("Model failed.")
        banner.refreshFailed("Refresh failed.", token: 1)
        banner.setModel(nil)
        #expect(banner.announcement == nil)
        banner.refreshSucceeded(token: 2)
        #expect(banner.visible == nil && banner.announcement == nil)
    }

    @Test("An older success cannot clear a newer failure, and the latest success clears equal-tree errors")
    func successOrder() {
        var banner = ChatBanner()
        banner.refreshFailed("Latest refresh failed.", token: 4)
        banner.refreshSucceeded(token: 3)
        #expect(banner.visible?.text == "Latest refresh failed.")
        banner.refreshSucceeded(token: 5)
        #expect(banner.visible == nil && banner.announcement == nil)
        banner.refreshSucceeded(token: 2)
        banner.refreshFailed("Old refresh failed.", token: 4)
        #expect(banner.visible == nil)
    }
}
