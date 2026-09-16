import Observation
import BloomCore

/// Whether the Ask Bloom panel is in front of the window.
///
/// Shared rather than held by a view, for the reason `SearchPanelModel` is: the panel is drawn by
/// a hosting view over the window's frame view, and that host's hit test has to ask whether there
/// is anything up without being a SwiftUI view that could read an environment.
@MainActor
@Observable
final class AskPanelModel {
    static let shared = AskPanelModel()

    private(set) var isOpen = false

    private init() {}

    func open(app: AppModel) {
        SearchPanelModel.shared.close(app: app)
        isOpen = true
    }

    func close() {
        guard isOpen else { return }
        isOpen = false
    }

    func toggle(app: AppModel) {
        if isOpen { close() } else { open(app: app) }
    }
}

extension AppModel {
    /// Whether the tab commands (New Tab, Close Tab, the tab cycle and Go to Tab) are about Ask
    /// Bloom's conversations rather than the selected workspace's tabs: the panel is in front of
    /// the window. With it up, Command-W closing a tab of the workspace hidden behind it would be
    /// closing something nobody is looking at.
    var isAskInFront: Bool { AskPanelModel.shared.isOpen }
}
