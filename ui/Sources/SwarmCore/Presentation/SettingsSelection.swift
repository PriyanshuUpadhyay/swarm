import Foundation
import Observation

/// All Settings entry points share one page and save it through the choices file's lock.
@MainActor @Observable
public final class SettingsSelection {
    public private(set) var page = SettingsPage.profiles
    public private(set) var prefs = Prefs()
    // Each source clears and announces only its own error.
    public private(set) var appError: String?
    public private(set) var loadError: String?
    public private(set) var saveError: String?
    private var pageErrors: [SettingsPage: String] = [:]
    public var pageError: String? {
        page == .setup ? ErrorAnnouncement.joined([skillsRefreshError, pageErrors[page]]) : pageErrors[page]
    }
    public private(set) var skillsRefreshError: String?
    public private(set) var skillsReady = false
    public private(set) var skillsRefreshing = false
    @ObservationIgnored private var skillsRefreshTask: Task<Bool, Never>?
    @ObservationIgnored private var skillsRefreshOperation: (@Sendable () async throws -> Void)?
    @ObservationIgnored private var skillsStartupReady = false
    public private(set) var appErrorRevision = 0
    public private(set) var loadErrorRevision = 0
    public private(set) var saveErrorRevision = 0
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
            if saveError == nil {
                changePage(saved.prefs.settingsPage.flatMap(SettingsPage.init(rawValue:)) ?? .profiles)
            }
            loadError = nil
        } catch {
            loadError = OwnerChoicesFailure(error.localizedDescription, operation: .load).message
            loadErrorRevision += 1
        }
    }

    public func select(_ page: SettingsPage) {
        changePage(page)
        do {
            prefs = try choices.update { $0.prefs.settingsPage = page.rawValue }.prefs
            saveError = nil
        } catch {
            saveError = OwnerChoicesFailure(error.localizedDescription, operation: .saveSettings).message
            saveErrorRevision += 1
        }
    }

    private func changePage(_ page: SettingsPage) {
        if self.page != page { setPageError(nil, on: self.page) }
        self.page = page
    }

    /// Updates a page's stored text without announcing a state change or clear.
    public func setPageError(_ message: String?, on page: SettingsPage) {
        pageErrors[page] = message
    }

    /// Reports a new failure, including an identical one, on its source page.
    public func reportPageError(_ message: String, on page: SettingsPage) {
        pageErrors[page] = message
        if self.page == page { pageErrorRevision += 1 }
    }

    public func setAppError(_ message: String?) {
        appError = message
        if message != nil { appErrorRevision += 1 }
    }

    /// One startup task lets every setup reader wait for the same home refresh.
    public func beginSkillsRefresh(
        after: @escaping @MainActor () async -> Bool,
        refresh: @escaping @Sendable () async throws -> Void
    ) {
        guard skillsRefreshTask == nil else { return }
        skillsRefreshOperation = refresh
        skillsRefreshing = true
        skillsRefreshTask = Task {
            guard await after() else {
                skillsRefreshing = false
                return false
            }
            skillsStartupReady = true
            return await performSkillsRefresh(refresh)
        }
    }

    public func waitForSkillsRefresh() async -> Bool {
        await skillsRefreshTask?.value ?? false
    }

    public func retrySkillsRefresh() async -> Bool {
        if skillsRefreshing { return await waitForSkillsRefresh() }
        guard skillsStartupReady, let refresh = skillsRefreshOperation else { return false }
        skillsRefreshing = true
        skillsRefreshTask = Task { await performSkillsRefresh(refresh) }
        return await waitForSkillsRefresh()
    }

    private func performSkillsRefresh(_ refresh: @Sendable () async throws -> Void) async -> Bool {
        defer { skillsRefreshing = false }
        do {
            try await refresh()
            skillsRefreshError = nil
            skillsReady = true
            return true
        } catch {
            skillsReady = false
            skillsRefreshError = "Could not refresh skills. " + ErrorText.sentence(error.localizedDescription)
            if page == .setup { pageErrorRevision += 1 }
            return false
        }
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

    @discardableResult
    public func setSkillsCheckout(_ path: String?) -> Bool {
        let canonical: String?
        do {
            canonical = try path.flatMap { $0.isEmpty ? nil : try SkillCheckout.validate(path: $0) }
        } catch {
            reportPageError(error.localizedDescription, on: .skills)
            return false
        }
        if save(on: .skills, { $0.skillsCheckout = canonical }) {
            setPageError(nil, on: .skills)
            return true
        }
        return false
    }

    @discardableResult
    private func save(on page: SettingsPage? = nil, _ change: (inout Prefs) -> Void) -> Bool {
        let previous = prefs
        change(&prefs)
        do {
            prefs = try choices.update { change(&$0.prefs) }.prefs
            saveError = nil
            return true
        } catch {
            prefs = previous
            let message = OwnerChoicesFailure(error.localizedDescription, operation: .saveSettings).message
            if let page { reportPageError(message, on: page) }
            else { saveError = message; saveErrorRevision += 1 }
            return false
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
