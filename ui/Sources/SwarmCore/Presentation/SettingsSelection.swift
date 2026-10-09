import Foundation
import Observation

/// All Settings entry points share one page and save it through the choices file's lock.
@MainActor @Observable
public final class SettingsSelection {
    public private(set) var page = SettingsPage.profiles
    public private(set) var prefs = Prefs()
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
            error = nil
        } catch {
            self.error = OwnerChoicesFailure(error.localizedDescription, operation: .load).message
        }
    }

    public func select(_ page: SettingsPage) {
        self.page = page
        do {
            prefs = try choices.update { $0.prefs.settingsPage = page.rawValue }.prefs
            error = nil
        } catch {
            self.error = OwnerChoicesFailure(error.localizedDescription, operation: .save).message
        }
    }

    public func setError(_ message: String?) {
        error = message
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
        update(&prefs)
        do {
            prefs = try choices.update { update(&$0.prefs) }.prefs
            error = nil
        } catch {
            self.error = OwnerChoicesFailure(error.localizedDescription, operation: .save).message
        }
    }
}
