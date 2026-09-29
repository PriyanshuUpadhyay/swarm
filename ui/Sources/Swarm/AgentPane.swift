import Observation
import SwiftUI
import SwarmCore

/// Owns the child columns of the selected chat: one chat model per agent, kept while the chat is
/// selected, also when its column scrolls off screen (ADR 0024, ADR 0029). A column reads its log
/// only while it is on screen.
@MainActor @Observable
final class AgentPaneStore {
    private var columns: [String: ChildColumnModel] = [:]
    private(set) var focusedKey: String?
    /// The pane that fills the main area. Keys set it; the strip only shows it.
    var zoomedKey: String?
    /// The pane a key last moved focus to, or nil for the chat page; the strip scrolls to it.
    private(set) var revealKey: String?
    /// Bumped on every reveal, so revealing the chat again scrolls even when revealKey was nil.
    private(set) var revealCount = 0

    func clearFocus() { focusedKey = nil }

    static func key(session: SwarmSessionID, agent: String) -> String {
        session.rawValue + ":" + agent
    }

    func column(key: String) -> ChildColumnModel {
        if let column = columns[key] { return column }
        let column = ChildColumnModel()
        columns[key] = column
        return column
    }

    /// Focuses the column's composer and scrolls the strip to it.
    func focus(key: String) {
        focusedKey = key
        revealKey = key
        revealCount += 1
    }

    /// The column's own composer took focus, as by a click.
    func focused(key: String) { focusedKey = key }

    /// Moves focus among `keys`, in strip order. Returns false when focus lands on the chat page,
    /// which the caller focuses.
    func moveFocus(_ direction: FocusDirection, among keys: [String]) -> Bool {
        let current = keys.firstIndex { $0 == focusedKey }.map(PaneStripLayout.Focus.pane) ?? .chat
        switch PaneStripLayout.move(from: current, count: keys.count, direction: direction) {
        case .chat:
            revealChat()
            return false
        case .pane(let index):
            focus(key: keys[index])
            return true
        }
    }

    /// Zooms `key` (by default the focused pane), or returns the zoomed pane to the strip. Keys
    /// and the header button both come here.
    func toggleZoom(key requested: String? = nil) {
        guard let key = requested ?? zoomedKey ?? focusedKey else { return }
        zoomedKey = zoomedKey == key ? nil : key
        focusedKey = key
    }

    /// The strip shows the chat page again, unzoomed, as when focus moves to it.
    func revealChat() {
        zoomedKey = nil
        revealKey = nil
        focusedKey = nil
        revealCount += 1
    }

    /// Frees every column outside `session`, so a chat or workspace switch stops its readers.
    func stop(keepingSession session: SwarmSessionID?) {
        let kept = session.map { $0.rawValue + ":" }
        for key in columns.keys where kept.map({ !key.hasPrefix($0) }) ?? true {
            columns.removeValue(forKey: key)
            if focusedKey == key { focusedKey = nil }
            if zoomedKey == key { zoomedKey = nil }
            if revealKey == key { revealKey = nil }
        }
    }

    func stopAll() { stop(keepingSession: nil) }
}
