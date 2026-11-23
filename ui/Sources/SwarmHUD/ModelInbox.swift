import Foundation
import SwarmHUDCore

/// ADR 0147: the inbox actions of the open card and the popup.
extension UsageModel {
    /// Closes the HUD at once, then opens the chat where it lives (the app, or its terminal) and
    /// marks its rows seen. A chat that does not open says why aloud and in the card's error line.
    func openChat(session: String, agent: String?) {
        closeHUD?()
        guard opening.insert(session).inserted else { return }
        inboxChanging()
        let swarm = swarm
        Task {
            // `focus` takes the chair's pane when the agent's pane is gone. A chat that did not open
            // is not seen, so it stays in the inbox.
            let focused = await offPool { run(swarm, ["focus", session, agent ?? "orchestrator"], timeout: 5) }
            opening.remove(session)
            if case .failure(let error) = focused {
                actionError = "Could not open: \(error.text)"
                inboxChanged()
                return
            }

            popup?.items.removeAll { $0.session == session }
            if popup?.items.isEmpty == true { popup = nil }
            let seen = await offPool { run(swarm, ["notices", "seen", session], timeout: 3) }
            if case .failure(let error) = seen { actionError = "Could not mark it seen: \(error.text)" } else { actionError = nil }
            inboxChanged()
        }
    }

    /// `swarm answer` refuses when the prompt on screen is no longer `prompt.id`.
    func answer(_ need: NeedCard, choice: Int) {
        guard let prompt = need.prompt, answering.insert(need.key).inserted else { return }
        let environment = ["SWARM_SESSION_ID": need.session].merging(need.adapter.map { ["SWARM_ADAPTER": $0] } ?? [:]) { $1 }
        inboxChanging()
        let swarm = swarm
        Task {
            let result = await offPool {
                run(swarm, ["answer", need.agent, prompt.id, String(choice)], timeout: 10, environment: environment)
            }
            if case .failure(let error) = result { actionError = "Could not answer: \(error.text)" } else { actionError = nil }
            answering.remove(need.key)
            inboxChanged()
        }
    }

    /// Marks the chats of this read seen, also those under "+N more", so the inbox list and the
    /// closed wing's count empty. Only rows up to the newest one read are marked, so a row that came
    /// after this read stays.
    func clearInbox() {
        let sessions = inbox.rows.map(\.session)
        guard let through = inbox.rows.map(\.id).max() else { return }
        inbox.rows = []
        inbox.rowsTotal = 0
        popup = nil
        inboxChanging()
        let swarm = swarm
        Task {
            let seen = await offPool { run(swarm, ["notices", "seen", "--through", String(through)] + sessions, timeout: 3) }
            if case .failure(let error) = seen { actionError = "Could not clear: \(error.text)" } else { actionError = nil }
            inboxChanged()
        }
    }
}
