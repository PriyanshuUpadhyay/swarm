// ADR 0156: the HUD reads its chats when `swarm` signals a store change, not on a fast poll.
import AppKit
import notify

/// A turn end writes the state and then the notice, so the signals of one burst wait this long and
/// cost one read.
private let changeDelay: TimeInterval = 0.25

extension AppDelegate {
    /// Starts the reads of the working agents and the inbox: now, on each change signal, and each minute.
    func startChatReads() {
        model.readWorking()
        model.readInbox()
        listenForChanges()
        // swarm signals each change, so this poll only catches what no write marks, such as a
        // `working` report that grows old, and a signal that was lost.
        workingTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { _ in
            MainActor.assumeIsolated {
                self.model.readWorking()
                self.model.readInbox()
            }
        }
        workingTimer?.tolerance = 5
    }

    /// ADR 0156: `swarm` posts this Darwin notify name (`hud::CHANGED`) when an agent's state, a pane,
    /// or a notice changes.
    private func listenForChanges() {
        let status = notify_register_dispatch("io.github.priyanshuupadhyay.swarm.changed", &changeToken, .main) { _ in
            MainActor.assumeIsolated { self.storeChanged() }
        }
        // The 60 s poll still reads, so a failed registration only makes the HUD slow.
        if status != NOTIFY_STATUS_OK { NSLog("SwarmHUD: notify_register_dispatch failed: \(status)") }
    }

    private func storeChanged() {
        guard pendingChange == nil else { return }
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            pendingChange = nil
            model.readWorking()
            model.readInbox()
        }
        pendingChange = item
        DispatchQueue.main.asyncAfter(deadline: .now() + changeDelay, execute: item)
    }
}
