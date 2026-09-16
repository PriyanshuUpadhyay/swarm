import Foundation

/// Asks the Settings window to show one pane.
///
/// The window picks its pane at launch from `Snapshot.requestedSettingsTab`, which is a command
/// line flag and therefore no use to a menu item. This is the runtime half: the menu bar's
/// "Menubar Settings…" posts a tab, `SettingsView` listens, and the window opens on the pane the
/// person asked for rather than on whichever one they left it on.
enum SettingsTabRequest {
    static let name = Notification.Name("be.spatie.bloom.settings.tab")

    /// The tab asked for most recently, kept until the Settings window reads it as it appears. A
    /// notification posted before the window exists reaches nobody, so "Manage presets…" opened
    /// Settings on whichever pane it was left on the first time it was pressed.
    @MainActor private static var pending: SettingsTab?

    @MainActor static func takePending() -> SettingsTab? {
        defer { pending = nil }
        return pending
    }

    @MainActor static func post(_ tab: SettingsTab) {
        pending = tab
        NotificationCenter.default.post(name: name, object: nil, userInfo: ["tab": tab.rawValue])
    }

    static func tab(in notification: Notification) -> SettingsTab? {
        (notification.userInfo?["tab"] as? String).flatMap(SettingsTab.init(rawValue:))
    }
}
