import Foundation
import Testing
@testable import SwarmCore

@Suite("Settings selection")
@MainActor
struct SettingsSelectionTests {
    @Test("Opening a page saves it in choices and another selection reads it")
    func savedPage() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        try choices.update { $0.pinned = ["/project"] }
        let selection = SettingsSelection(choices: choices)
        #expect(selection.page == .profiles)
        selection.select(.managedChanges)
        #expect(selection.error == nil)
        #expect(try choices.load().prefs.settingsPage == "managedChanges")
        #expect(try choices.load().pinned == ["/project"])
        #expect(SettingsSelection(choices: choices).page == .managedChanges)
        try choices.update { $0.prefs.settingsPage = "future-page" }
        selection.reload()
        #expect(selection.page == .profiles)
        #expect(try choices.load().prefs.settingsPage == "future-page")
    }

    @Test("A failed save reports the error and keeps the selected page visible")
    func failedSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        #expect(selection.page == .setup)
        #expect(selection.error?.contains("Could not save") == true)
    }
}
