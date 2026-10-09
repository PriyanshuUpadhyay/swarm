import Foundation
import Observation

/// All Settings entry points share one page and save it through the choices file's lock.
@MainActor @Observable
public final class SettingsSelection {
    public private(set) var page = SettingsPage.profiles
    public private(set) var prefs = Prefs()
    // Each source clears and announces only its own error.
    public private(set) var appError: String?
    public private(set) var storeError: String?
    public private(set) var pageError: String?
    public private(set) var appErrorRevision = 0
    public private(set) var storeErrorRevision = 0
    public private(set) var pageErrorRevision = 0
    private let choices: OwnerChoicesStore

    public init(choices: OwnerChoicesStore = OwnerChoicesStore()) {
        self.choices = choices
        reload()
    }

    public func reload() {
        do {
            let saved = try choices.load(waitForLock: true)
            prefs = saved.prefs
            page = saved.prefs.settingsPage.flatMap(SettingsPage.init(rawValue:)) ?? .profiles
            setStoreError(nil)
        } catch {
            setStoreError(OwnerChoicesFailure(error.localizedDescription, operation: .load).message)
        }
    }

    public func select(_ page: SettingsPage) {
        self.page = page
        do {
            prefs = try choices.update { $0.prefs.settingsPage = page.rawValue }.prefs
            setStoreError(nil)
        } catch {
            setStoreError(OwnerChoicesFailure(error.localizedDescription, operation: .saveSettings).message)
        }
    }

    public func setError(_ message: String?) {
        pageError = message
        if message != nil { pageErrorRevision += 1 }
    }

    public func setAppError(_ message: String?) {
        appError = message
        if message != nil { appErrorRevision += 1 }
    }

    private func setStoreError(_ message: String?) {
        storeError = message
        if message != nil { storeErrorRevision += 1 }
    }

    public func setSplitDiff(_ split: Bool) {
        save { $0.splitDiff = split }
    }

    public func setTheme(_ theme: Theme) {
        save { $0.theme = theme }
    }

    public func setTextSize(_ textSize: TextSize) {
        save { $0.textSize = textSize }
    }

    public func setDensity(_ density: Density) {
        save { $0.density = density }
    }

    public func setSendKey(_ sendKey: SendKey) {
        save { $0.sendKey = sendKey }
    }

    private func save(_ change: (inout Prefs) -> Void) {
        let previous = prefs
        change(&prefs)
        do {
            prefs = try choices.update { change(&$0.prefs) }.prefs
            setStoreError(nil)
        } catch {
            prefs = previous
            setStoreError(OwnerChoicesFailure(error.localizedDescription, operation: .saveSettings).message)
        }
    }

    public func setNotice(_ field: WritableKeyPath<NoticePrefs, Bool>, to value: Bool) {
        saveNotices { $0[keyPath: field] = value }
    }

    public func setProjectMuted(_ path: String, to muted: Bool) {
        saveNotices {
            if muted { $0.mutedProjects.insert(path) }
            else { $0.mutedProjects.remove(path) }
        }
    }

    private func saveNotices(_ change: (inout NoticePrefs) -> Void) {
        save { change(&$0.notices) }
    }
}
