import Foundation
import Testing
@testable import SwarmCore

@Suite("Settings selection")
@MainActor
struct SettingsSelectionTests {
    @Test("Page and app errors repeat only their own announcements and clears do not announce")
    func independentAnnouncements() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.setAppError("Notices are off.")
        selection.setError("Guards failed")
        let appRevision = selection.appErrorRevision
        let pageRevision = selection.pageErrorRevision
        let storeRevision = selection.storeErrorRevision
        selection.setError("Guards failed")
        #expect(selection.pageErrorRevision == pageRevision + 1)
        #expect(selection.appErrorRevision == appRevision)
        #expect(selection.storeErrorRevision == storeRevision)
        selection.setAppError("Notices are off.")
        #expect(selection.appErrorRevision == appRevision + 1)
        #expect(selection.pageErrorRevision == pageRevision + 1)
        selection.setError(nil)
        selection.setAppError(nil)
        selection.setError(nil)
        #expect(selection.pageErrorRevision == pageRevision + 1)
        #expect(selection.appErrorRevision == appRevision + 1)
        #expect(selection.storeErrorRevision == storeRevision)
        #expect(selection.pageError == nil && selection.appError == nil)
    }

    @Test("Joined messages have a stop between sentences without adding a second period")
    func joinedMessages() {
        #expect(ErrorAnnouncement(messages: ["Guards failed", "Setup failed."], revision: 0).text == "Guards failed. Setup failed.")
        #expect(ErrorAnnouncement(messages: ["Notices are off.", nil, "", "Guards failed", "Setup failed"], revision: 0).text == "Notices are off. Guards failed. Setup failed")
        #expect(ErrorAnnouncement(messages: [nil, ""], revision: 0).text == nil)
    }

    @Test("Page clears keep load and save failures until a good store operation")
    func storeErrorsHaveTheirOwnSlot() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        let selection = SettingsSelection(choices: choices)
        selection.setAppError("App failed.")
        selection.setError("Page failed.")
        let appRevision = selection.appErrorRevision
        let pageRevision = selection.pageErrorRevision
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        selection.reload()
        let loadError = selection.storeError
        let loadRevision = selection.storeErrorRevision
        #expect(loadError?.hasPrefix("Could not load") == true)
        #expect(selection.pageError == "Page failed.")
        selection.setError(nil)
        #expect(selection.storeError == loadError)
        #expect(selection.storeErrorRevision == loadRevision)
        selection.reload()
        #expect(selection.storeErrorRevision == loadRevision + 1)
        selection.setTheme(.dark)
        #expect(selection.storeError?.hasPrefix("Could not save") == true)
        #expect(selection.storeErrorRevision == loadRevision + 2)
        #expect(selection.appErrorRevision == appRevision && selection.pageErrorRevision == pageRevision)
        selection.setAppError(nil)
        #expect(selection.storeError != nil)
        try FileManager.default.removeItem(at: lock)
        selection.setError("Page failed again.")
        selection.setTheme(.dark)
        #expect(selection.storeError == nil)
        #expect(selection.pageError == "Page failed again.")
        #expect(selection.storeErrorRevision == loadRevision + 2)
    }

    @Test("Opening a page saves it in choices and another selection reads it")
    func savedPage() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        try choices.update { $0.pinned = ["/project"] }
        let selection = SettingsSelection(choices: choices)
        #expect(selection.page == .profiles)
        selection.select(.managedChanges)
        #expect(selection.storeError == nil)
        #expect(try choices.load().prefs.settingsPage == "managedChanges")
        #expect(try choices.load().pinned == ["/project"])
        #expect(SettingsSelection(choices: choices).page == .managedChanges)
        try choices.update { $0.prefs.settingsPage = "future-page" }
        selection.reload()
        #expect(selection.page == .profiles)
        #expect(try choices.load().prefs.settingsPage == "future-page")
    }

    @Test("Page clears, selection, saves, and reloads keep the app error until Dismiss")
    func appErrorPersists() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        let selection = SettingsSelection(choices: choices)
        let denial = "Notifications are off for Swarm."
        selection.setAppError(denial)
        selection.setError("Page failed.")
        selection.setError(nil)
        #expect(selection.appError == denial && selection.storeError == nil)
        #expect(selection.pageError == nil)
        selection.select(.setup)
        selection.setTheme(.dark)
        selection.setNotice(\.sound, to: false)
        selection.setProjectMuted("/project", to: true)
        selection.reload()
        #expect(selection.appError == denial && selection.storeError == nil)
        #expect(selection.pageError == nil)
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        selection.reload()
        #expect(selection.appError == denial)
        #expect(selection.storeError?.hasPrefix("Could not load") == true)
        selection.setAppError(nil)
        #expect(selection.appError == nil)
        #expect(selection.storeError?.hasPrefix("Could not load") == true)
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.storeError == nil)
    }

    @Test("A recovered load clears the store error and keeps page and app errors")
    func recoveredLoadError() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        let selection = SettingsSelection(choices: choices)
        #expect(selection.storeError?.hasPrefix("Could not load") == true)
        selection.setAppError("App lock failed.")
        selection.setError("Page failed.")
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.storeError == nil)
        #expect(selection.appError == "App lock failed.")
        #expect(selection.pageError == "Page failed.")
    }

    @Test("A failed preference save restores live values and keeps the persisted bytes")
    func failedPreferenceSave() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        try choices.update {
            $0.prefs.theme = .dark
            $0.prefs.textSize = .large
            $0.prefs.density = .compact
            $0.prefs.sendKey = .commandReturn
            $0.prefs.splitDiff = true
            $0.prefs.notices.sound = false
        }
        let selection = SettingsSelection(choices: choices)
        let previous = selection.prefs
        let file = folder.appendingPathComponent("choices.json")
        let saved = try Data(contentsOf: file)
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        selection.setTheme(.light)
        #expect(selection.prefs == previous)
        selection.setTextSize(.small)
        #expect(selection.prefs == previous)
        selection.setDensity(.comfortable)
        #expect(selection.prefs == previous)
        selection.setSendKey(.return)
        #expect(selection.prefs == previous)
        selection.setSplitDiff(false)
        #expect(selection.prefs == previous)
        selection.setNotice(\.post, to: false)
        #expect(selection.prefs == previous)
        selection.setProjectMuted("/project", to: true)
        #expect(selection.prefs == previous)
        #expect(selection.storeError?.hasPrefix("Could not save your settings.") == true)
        #expect(try Data(contentsOf: file) == saved)
        let saveError = selection.storeError
        let storeRevision = selection.storeErrorRevision
        selection.setError("Page failed.")
        selection.setError(nil)
        #expect(selection.storeError == saveError)
        #expect(selection.storeErrorRevision == storeRevision)
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.prefs == previous)
        #expect(selection.storeError == nil)
        #expect(selection.storeErrorRevision == storeRevision)
        selection.setError("Page failed.")
        selection.setTheme(.light)
        #expect(selection.storeError == nil)
        #expect(selection.pageError == "Page failed.")
        #expect(selection.storeErrorRevision == storeRevision)
    }

    @Test("A failed save reports the error and keeps the selected page visible")
    func failedSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        #expect(selection.page == .setup)
        #expect(selection.storeError?.hasPrefix("Could not save your settings.") == true)
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
        #expect(selection.storeError == nil)
        #expect(SettingsSelection(choices: choices).prefs.notices == saved.prefs.notices)
        selection.setNotice(\.sound, to: true)
        #expect(try choices.load().prefs.notices.sound)
    }

    @Test("A failed notice save restores the switch and reports a settings error")
    func failedNoticeSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.setNotice(\.post, to: false)
        #expect(selection.prefs.notices.post)
        #expect(selection.storeError?.hasPrefix("Could not save your settings.") == true)
        selection.setProjectMuted("/project", to: true)
        #expect(selection.prefs.notices.mutedProjects.isEmpty)
        #expect(selection.storeError?.hasPrefix("Could not save your settings.") == true)
    }
}
