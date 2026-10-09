import Foundation
import Observation

/// All Settings entry points share one page and save it through the choices file's lock.
@MainActor @Observable
public final class SettingsSelection {
    public private(set) var page = SettingsPage.profiles
    public private(set) var prefs = Prefs()
    // Page loads and saves clear only the page error.
    // App errors stay until the owner presses their Dismiss button.
    public private(set) var appError: String?
    public private(set) var error: String?
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
            setError(nil)
        } catch {
            setError(OwnerChoicesFailure(error.localizedDescription, operation: .load).message)
        }
    }

    public func select(_ page: SettingsPage) {
        self.page = page
        do {
            prefs = try choices.update { $0.prefs.settingsPage = page.rawValue }.prefs
            setError(nil)
        } catch {
            setError(OwnerChoicesFailure(error.localizedDescription, operation: .saveSettings).message)
        }
    }

    public func setError(_ message: String?) {
        error = message
    }

    public func setAppError(_ message: String?) {
        appError = message
    }

    public func setSplitDiff(_ split: Bool) {
        updatePrefs { $0.splitDiff = split }
    }

    public func setTheme(_ theme: Theme) {
        updatePrefs { $0.theme = theme }
    }

    public func setTextSize(_ textSize: TextSize) {
        updatePrefs { $0.textSize = textSize }
    }

    public func setDensity(_ density: Density) {
        updatePrefs { $0.density = density }
    }

    public func setSendKey(_ sendKey: SendKey) {
        updatePrefs { $0.sendKey = sendKey }
    }

    private func updatePrefs(_ update: (inout Prefs) -> Void) {
        save(update)
    }

    private func save(_ change: (inout Prefs) -> Void) {
        let previous = prefs
        change(&prefs)
        do {
            prefs = try choices.update { change(&$0.prefs) }.prefs
            setError(nil)
        } catch {
            prefs = previous
            setError(OwnerChoicesFailure(error.localizedDescription, operation: .saveSettings).message)
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
