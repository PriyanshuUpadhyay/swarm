import Foundation
import SwarmHUDCore

/// ADR 0147: the inbox actions of the open card and the popup.
extension UsageModel {
    /// Opens the chat where it lives (the app, or its terminal) and marks its rows seen. `line` is the
    /// key of the popup line that asked, which shows the error when the chat does not open.
    func openChat(session: String, agent: String?, line: String? = nil) {
        guard opening.insert(session).inserted else { return }
        inboxChanging()
        let swarm = swarm
        Task {
            // `focus` takes the chair's pane when the agent's pane is gone. A chat that did not open
            // is not seen, so it stays in the inbox, and its popup line says why.
            let focused = await offPool { run(swarm, ["focus", session, agent ?? "orchestrator"], timeout: 5) }
            opening.remove(session)
            if case .failure(let error) = focused {
                // Setting actionError says it aloud, so the popup line does not.
                let message = "Could not open: \(error.text)"
                actionError = message
                if let line, popup?.items.contains(where: { $0.key == line }) == true {
                    showPopupError(line, message)
                }
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

    /// Shows the error for `Popup.peek`, and keeps a done popup up that long too.
    func showPopupError(_ line: String, _ text: String) {
        popupError = (line, text)
        popup?.lastAt = Date()
        popupErrorTimer?.cancel()
        popupErrorTimer = Task {
            try? await Task.sleep(for: .seconds(Popup.peek))
            guard !Task.isCancelled else { return }
            popupError = nil
            closeExpiredPopup()
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
