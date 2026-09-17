import Foundation
import Observation

/// The one confirmation used by a swarm agent tab's close button and by Cmd+W.
@MainActor
@Observable
final class SwarmAgentCloseAlert {
    static let shared = SwarmAgentCloseAlert()

    struct Request: Identifiable, Equatable {
        let id = UUID()
        var tab: CenterTab
        var model: WorkspaceModel

        static func == (lhs: Request, rhs: Request) -> Bool { lhs.id == rhs.id }
    }

    var request: Request?

    func ask(_ tab: CenterTab, in model: WorkspaceModel) {
        request = Request(tab: tab, model: model)
    }

    func confirm() {
        guard let request else { return }
        self.request = nil
        Task { await CenterTabStore.shared.close(request.tab, in: request.model) }
    }

    func cancel() {
        request = nil
    }
}
