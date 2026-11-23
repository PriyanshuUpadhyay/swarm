import AppKit
import SwarmHUDCore

/// The right-click menu of the HUD.
extension AppDelegate {
    func contextMenu() -> NSMenu {
        let menu = NSMenu()
        // Else the menu enables every item whose target answers, and the refresh state is lost.
        menu.autoenablesItems = false
        let refresh = NSMenuItem(title: "Refresh usage", action: #selector(refreshUsage), keyEquivalent: "r")
        refresh.target = self
        refresh.isEnabled = model.canRefresh
        menu.addItem(refresh)
        menu.addItem(.separator())
        let theme = NSMenuItem(title: "Theme", action: nil, keyEquivalent: "")
        let themes = NSMenu()
        for (title, phase) in [("Follow the clock", nil)] + DayPhase.allCases.map({ ($0.title, Optional($0)) }) {
            let item = NSMenuItem(title: title, action: #selector(pickTheme(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = phase?.rawValue
            item.state = model.pinnedPhase == phase ? .on : .off
            themes.addItem(item)
        }
        theme.submenu = themes
        menu.addItem(theme)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Usage HUD", action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
        return menu
    }

    @objc func refreshUsage() { model.refresh() }

    @objc func pickTheme(_ item: NSMenuItem) {
        model.pinnedPhase = (item.representedObject as? String).flatMap(DayPhase.init(rawValue:))
    }

    /// A clean exit, so the LaunchAgent (KeepAlive.SuccessfulExit = false) does not restart it.
    @objc func quit() { NSApp.terminate(nil) }
}
