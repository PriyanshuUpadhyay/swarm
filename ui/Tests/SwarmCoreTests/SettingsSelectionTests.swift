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

    @Test("Notice switches and project mutes save one field and keep other choices")
    func notices() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        try choices.update { $0.pinned = ["/project"]; $0.prefs.splitDiff = true }
        let selection = SettingsSelection(choices: choices)
        try choices.update { $0.prefs.notices.sound = false }
        selection.setNotice(\.post, to: false)
        selection.setNotice(\.done, to: false)
        selection.setNotice(\.badge, to: false)
        selection.setProjectMuted("/project", to: true)
        selection.setProjectMuted("/other", to: true)
        selection.setProjectMuted("/other", to: false)
        let saved = try choices.load()
        #expect(saved.prefs.splitDiff && saved.pinned == ["/project"])
        #expect(!saved.prefs.notices.post && !saved.prefs.notices.sound && !saved.prefs.notices.done && !saved.prefs.notices.badge)
        #expect(saved.prefs.notices.mutedProjects == ["/project"])
        #expect(selection.error == nil)
        #expect(SettingsSelection(choices: choices).prefs.notices == saved.prefs.notices)
        selection.setNotice(\.sound, to: true)
        #expect(try choices.load().prefs.notices.sound)
    }

    @Test("A failed notice save keeps the switch choice and reports its error")
    func failedNoticeSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.setNotice(\.post, to: false)
        #expect(!selection.prefs.notices.post)
        #expect(selection.error?.contains("Could not save") == true)
        selection.setProjectMuted("/project", to: true)
        #expect(selection.prefs.notices.mutedProjects == ["/project"])
        #expect(selection.error?.contains("Could not save") == true)
    }
}
