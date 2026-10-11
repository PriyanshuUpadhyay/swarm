import SwarmCore

@MainActor
struct ChildAgentActions {
    let bus: any SwarmBus
    let session: SwarmSession
    let confirm: (SwarmAgentID) -> Void
    let error: (String?) -> Void

    func requestChildClose(_ id: SwarmAgentID) async {
        do {
            let agents = try await bus.agents(in: session)
            error(nil)
            guard let agent = agents.first(where: { $0.id == id }), agent.status != .ended else { return }
            if SwarmAgentCell(agent: agent).requiresCloseConfirmation { confirm(id) }
            else { await performChildAction(id, close: true) }
        } catch { self.error(error.localizedDescription) }
    }

    func performChildAction(_ id: SwarmAgentID, close: Bool) async {
        do {
            if close { try await bus.close(id, in: session) }
            else { try await bus.interrupt(id, in: session) }
            error(nil)
        } catch { self.error(error.localizedDescription) }
    }
}
