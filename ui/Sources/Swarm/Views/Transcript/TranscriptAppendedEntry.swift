import SwiftUI
import SwarmCore

extension TranscriptTableEntry {
    /// Rows a pane adds under the conversation, drawn in the transcript's own scroll.
    ///
    /// It exists for the swarm session, where the chair talks in one place and the agents it ran
    /// report in another. The agents used to be a column beside the chat, with a disclosure and a
    /// message box on every row; what a reader actually wants from them is the summary each one
    /// left when it closed, next to the chair's words that asked for it. A column cannot sit next
    /// to a sentence, so the rows come into the scroll instead.
    ///
    /// **The content key is fixed and the cell redraws itself.** What goes in here comes from a
    /// source the transcript does not read, so there is nothing for this pass to hash: a swarm
    /// agent's summary arrives on the bus rather than in the chair's log. `TranscriptEntryID`
    /// puts `appended` with the four singletons for that reason, and an `@Observable` inside the
    /// closure is what brings a new summary to the screen.
    static func appended(@ViewBuilder content: @escaping @MainActor () -> some View) -> Self {
        Self(
            id: .appended,
            contentKey: TranscriptContentKey { $0.combine("appended") },
            content: { AnyView(content()) }
        )
    }
}
