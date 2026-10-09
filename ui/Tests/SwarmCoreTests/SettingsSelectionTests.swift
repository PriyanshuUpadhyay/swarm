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
        #expect(selection.appError == denial && selection.error == nil)
        selection.select(.setup)
        selection.setTheme(.dark)
        selection.setNotice(\.sound, to: false)
        selection.setProjectMuted("/project", to: true)
        selection.reload()
        #expect(selection.appError == denial && selection.error == nil)
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        selection.reload()
        #expect(selection.appError == denial)
        #expect(selection.error?.hasPrefix("Could not load") == true)
        selection.setAppError(nil)
        #expect(selection.appError == nil)
        #expect(selection.error?.hasPrefix("Could not load") == true)
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.error == nil)
    }

    @Test("A recovered load clears the page error and keeps a later app error")
    func recoveredLoadError() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        let selection = SettingsSelection(choices: choices)
        #expect(selection.error?.hasPrefix("Could not load") == true)
        selection.setAppError("App lock failed.")
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.error == nil)
        #expect(selection.appError == "App lock failed.")
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
        #expect(selection.error?.hasPrefix("Could not save your settings.") == true)
        #expect(try Data(contentsOf: file) == saved)
        let saveError = selection.error
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.prefs == previous)
        #expect(selection.error == saveError)
        selection.setTheme(.light)
        #expect(selection.error == nil)
    }

    @Test("A failed save reports the error and keeps the selected page visible")
    func failedSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        #expect(selection.page == .setup)
        #expect(selection.error?.hasPrefix("Could not save your settings.") == true)
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

    @Test("A failed notice save restores the switch and reports a settings error")
    func failedNoticeSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.setNotice(\.post, to: false)
        #expect(selection.prefs.notices.post)
        #expect(selection.error?.hasPrefix("Could not save your settings.") == true)
        selection.setProjectMuted("/project", to: true)
        #expect(selection.prefs.notices.mutedProjects.isEmpty)
        #expect(selection.error?.hasPrefix("Could not save your settings.") == true)
    }
}
