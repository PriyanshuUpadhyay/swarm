import Foundation
import Testing
@testable import SwarmCore

@Suite("Settings selection")
@MainActor
struct SettingsSelectionTests {
    @Test("A good reload keeps a failed save and selecting another page clears its error")
    func errorRecoveryRegression() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        let saveError = selection.saveError
        let saveRevision = selection.saveErrorRevision
        #expect(saveError != nil)
        selection.reload()
        #expect(selection.page == .setup)
        #expect(selection.saveError == saveError && selection.loadError == nil)
        #expect(selection.saveErrorRevision == saveRevision)
        selection.setPageError("Setup failed.", on: selection.page)
        let pageRevision = selection.pageErrorRevision
        selection.select(.appearance)
        #expect(selection.pageError == nil)
        #expect(selection.pageErrorRevision == pageRevision)
        selection.select(.setup)
        #expect(selection.pageError == nil)
    }

    @Test("Page state stays silent, page events always announce, and clears do not announce")
    func independentAnnouncements() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.setAppError("Notices are off.")
        selection.setPageError("Guards failed", on: selection.page)
        let appRevision = selection.appErrorRevision
        let pageRevision = selection.pageErrorRevision
        #expect(pageRevision == 0)
        selection.setPageError("Guards failed", on: selection.page)
        #expect(selection.pageErrorRevision == pageRevision)
        selection.reportPageError("Guards failed", on: selection.page)
        #expect(selection.pageErrorRevision == pageRevision + 1)
        selection.reportPageError("Guards failed", on: selection.page)
        #expect(selection.pageErrorRevision == pageRevision + 2)
        selection.setPageError("Setup failed", on: selection.page)
        #expect(selection.pageErrorRevision == pageRevision + 2)
        selection.setAppError("Notices are off.")
        #expect(selection.appErrorRevision == appRevision + 1)
        selection.select(.profiles)
        #expect(selection.pageError == "Setup failed")
        #expect(selection.pageErrorRevision == pageRevision + 2)
        selection.setPageError(nil, on: selection.page)
        selection.setAppError(nil)
        selection.setPageError(nil, on: selection.page)
        #expect(selection.pageErrorRevision == pageRevision + 2)
        #expect(selection.appErrorRevision == appRevision + 1)
        #expect(selection.loadErrorRevision == 0 && selection.saveErrorRevision == 1)
        #expect(selection.pageError == nil && selection.appError == nil)
    }

    @Test("Late page failures and clears stay on the page that sent them")
    func latePageError() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        selection.reportPageError("Guards failed.", on: .setup)
        let revision = selection.pageErrorRevision
        selection.reportPageError("Profiles failed.", on: .profiles)
        #expect(selection.pageError == "Guards failed.")
        #expect(selection.pageErrorRevision == revision)
        selection.setPageError(nil, on: .profiles)
        #expect(selection.pageError == "Guards failed.")
        selection.reportPageError("Profiles failed again.", on: .profiles)
        selection.select(.profiles)
        #expect(selection.pageError == "Profiles failed again.")
        selection.select(.setup)
        #expect(selection.pageError == nil)
        selection.select(.profiles)
        #expect(selection.pageError == nil)
    }

    @Test("A hidden-page report stores text without announcing it on a later page switch")
    func hiddenPageReportStaysSilent() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        let revision = selection.pageErrorRevision
        selection.reportPageError("Setup failed.", on: .setup)
        #expect(selection.pageErrorRevision == revision && selection.pageError == nil)
        selection.select(.setup)
        #expect(selection.pageError == "Setup failed.")
        #expect(selection.pageErrorRevision == revision)
    }

    @Test("Clearing either joined source changes text without repeating the other failure")
    func quietJoinedState() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        selection.reportPageError("Guards failed. Setup failed.", on: .setup)
        let revision = selection.pageErrorRevision
        selection.setPageError("Guards failed.", on: .setup)
        #expect(selection.pageError == "Guards failed." && selection.pageErrorRevision == revision)
        selection.setPageError("Setup failed.", on: .setup)
        #expect(selection.pageError == "Setup failed." && selection.pageErrorRevision == revision)
        selection.setPageError(nil, on: .setup)
        #expect(selection.pageError == nil && selection.pageErrorRevision == revision)
    }

    @Test("Message sentences and joins add exactly one final period")
    func joinedMessages() {
        #expect(ErrorText.sentence("Timed out") == "Timed out.")
        #expect(ErrorText.sentence("Timed out.") == "Timed out.")
        #expect(ErrorAnnouncement.joined(["Guards failed", "Setup failed."]) == "Guards failed. Setup failed.")
        #expect(ErrorAnnouncement.joined(["Notices are off.", nil, "", "Guards failed", "Setup failed"]) == "Notices are off. Guards failed. Setup failed.")
        #expect(ErrorAnnouncement.joined([nil, ""]) == nil)
    }

    @Test("Load and save failures clear only after their own successful operation")
    func separateLoadAndSaveErrors() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        let selection = SettingsSelection(choices: choices)
        selection.setAppError("App failed.")
        selection.setPageError("Page failed.", on: selection.page)
        let appRevision = selection.appErrorRevision
        let pageRevision = selection.pageErrorRevision
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        selection.reload()
        let loadError = selection.loadError
        let loadRevision = selection.loadErrorRevision
        #expect(loadError?.hasPrefix("Could not load") == true)
        selection.setTheme(.dark)
        let saveError = selection.saveError
        let saveRevision = selection.saveErrorRevision
        #expect(saveError?.hasPrefix("Could not save") == true)
        #expect(selection.loadError == loadError && selection.loadErrorRevision == loadRevision)
        selection.setPageError(nil, on: selection.page)
        #expect(selection.loadError == loadError && selection.saveError == saveError)
        selection.reload()
        #expect(selection.loadErrorRevision == loadRevision + 1)
        #expect(selection.saveErrorRevision == saveRevision)
        selection.setTheme(.dark)
        #expect(selection.saveErrorRevision == saveRevision + 1)
        #expect(selection.loadErrorRevision == loadRevision + 1)
        #expect(selection.appErrorRevision == appRevision && selection.pageErrorRevision == pageRevision)
        try FileManager.default.removeItem(at: lock)
        selection.setPageError("Page failed again.", on: selection.page)
        selection.setTheme(.dark)
        #expect(selection.saveError == nil && selection.loadError == loadError)
        #expect(selection.saveErrorRevision == saveRevision + 1)
        selection.reload()
        #expect(selection.loadError == nil && selection.saveError == nil)
        #expect(selection.loadErrorRevision == loadRevision + 1)
        #expect(selection.pageError == "Page failed again." && selection.appError == "App failed.")
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
        #expect(selection.saveError == nil)
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
        selection.setPageError("Page failed.", on: selection.page)
        selection.setPageError(nil, on: selection.page)
        #expect(selection.appError == denial && selection.loadError == nil)
        #expect(selection.pageError == nil)
        selection.select(.setup)
        selection.setTheme(.dark)
        selection.setNotice(\.sound, to: false)
        selection.setProjectMuted("/project", to: true)
        selection.reload()
        #expect(selection.appError == denial && selection.loadError == nil)
        #expect(selection.pageError == nil)
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        selection.reload()
        #expect(selection.appError == denial)
        #expect(selection.loadError?.hasPrefix("Could not load") == true)
        selection.setAppError(nil)
        #expect(selection.appError == nil)
        #expect(selection.loadError?.hasPrefix("Could not load") == true)
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.loadError == nil)
    }

    @Test("A recovered load clears the load error and keeps errors on the same page")
    func recoveredLoadError() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        let lock = folder.appendingPathComponent("choices.lock")
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        let selection = SettingsSelection(choices: choices)
        #expect(selection.loadError?.hasPrefix("Could not load") == true)
        selection.setAppError("App lock failed.")
        selection.setPageError("Page failed.", on: selection.page)
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.loadError == nil)
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
        #expect(selection.saveError?.hasPrefix("Could not save your settings.") == true)
        #expect(try Data(contentsOf: file) == saved)
        let saveError = selection.saveError
        let saveRevision = selection.saveErrorRevision
        selection.setPageError("Page failed.", on: selection.page)
        selection.setPageError(nil, on: selection.page)
        #expect(selection.saveError == saveError)
        #expect(selection.saveErrorRevision == saveRevision)
        try FileManager.default.removeItem(at: lock)
        selection.reload()
        #expect(selection.prefs == previous)
        #expect(selection.saveError == saveError)
        #expect(selection.saveErrorRevision == saveRevision)
        selection.setPageError("Page failed.", on: selection.page)
        selection.setTheme(.light)
        #expect(selection.saveError == nil)
        #expect(selection.pageError == "Page failed.")
        #expect(selection.saveErrorRevision == saveRevision)
    }

    @Test("A failed save reports the error and keeps the selected page visible")
    func failedSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.select(.setup)
        #expect(selection.page == .setup)
        #expect(selection.saveError?.hasPrefix("Could not save your settings.") == true)
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
        #expect(selection.saveError == nil)
        #expect(SettingsSelection(choices: choices).prefs.notices == saved.prefs.notices)
        selection.setNotice(\.sound, to: true)
        #expect(try choices.load().prefs.notices.sound)
    }

    @Test("A failed notice save restores the switch and reports a settings error")
    func failedNoticeSave() {
        let selection = SettingsSelection(choices: OwnerChoicesStore(folder: nil))
        selection.setNotice(\.post, to: false)
        #expect(selection.prefs.notices.post)
        #expect(selection.saveError?.hasPrefix("Could not save your settings.") == true)
        selection.setProjectMuted("/project", to: true)
        #expect(selection.prefs.notices.mutedProjects.isEmpty)
        #expect(selection.saveError?.hasPrefix("Could not save your settings.") == true)
    }
}
