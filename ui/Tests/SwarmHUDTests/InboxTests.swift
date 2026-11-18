// Core tests for the notice inbox (ADR 0147): the read, the closed wing, and the popup.
import Foundation
import Testing
@testable import SwarmHUDCore

@Test func inboxReadsV1AndTheWingShowsANeedThenAStopThenTheDoneCount() throws {
    let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appending(path: "fixtures/inbox.json")
    let data = try Data(contentsOf: url)
    let inbox = try #require(InboxSnapshot.read(data))
    #expect(inbox.needs.first?.prompt?.id == "p1.req7")
    #expect(inbox.rows.map(\.kind) == [.asks, .done])
    // Another schema is no inbox, so the HUD keeps the last one rather than an empty one.
    let v2 = String(decoding: data, as: UTF8.self).replacingOccurrences(of: "swarm.inbox.v1", with: "swarm.inbox.v2")
    #expect(InboxSnapshot.read(Data(v2.utf8)) == nil)

    #expect(wingText(WorkingSnapshot(), best: nil, inbox: inbox)
            == WingText(left: "needs you", blocked: true, tags: ["Notice redesign"]))
    let stopped = InboxRow(id: 42, session: "s4", kind: .failed, title: "Docs", body: "API Error")
    let rows = InboxSnapshot(rows: inbox.rows + [stopped])
    #expect(wingText(WorkingSnapshot(working: ["claude": 2]), best: nil, inbox: rows) == WingText(left: "stopped", dot: .failed))
    // The store cuts its lists, so the wing counts from the full total.
    #expect(wingText(WorkingSnapshot(), best: nil, inbox: InboxSnapshot(rows: inbox.rows, rowsTotal: inbox.rowsTotal))
            == WingText(left: "7 done", dot: .done))
    #expect(inbox.needsTotal == 4)
}

@Test func aPopupShowsOnlyNewItemsJoinsABurstAndADoneOneCloses() {
    let at = Date(timeIntervalSince1970: 1_795_000_000)
    let done = InboxRow(id: 1, session: "a", kind: .done, title: "Chat A", body: "Done.")
    let asks = InboxRow(id: 2, session: "b", kind: .asks, title: "Chat B", body: "Should I land it?")
    // A HUD that starts shows nothing old.
    #expect(newPopupItems(InboxSnapshot(rows: [done]), known: nil).isEmpty)
    let first = InboxSnapshot(rows: [done])
    let new = newPopupItems(first, known: [])
    var popup = mergePopup(nil, new: new, now: at)
    #expect(popup?.items.map(\.word) == ["done"])
    #expect(popup?.expired(now: at.addingTimeInterval(Popup.peek), hovered: true) == false)
    #expect(popup?.expired(now: at.addingTimeInterval(Popup.peek), hovered: false) == true)

    // A question 5 s later joins the same popup, and a question waits for the owner.
    let second = InboxSnapshot(rows: [asks, done])
    popup = mergePopup(popup, new: newPopupItems(second, known: inboxKeys(first)), now: at.addingTimeInterval(5))
    #expect(popup?.items.map(\.title) == ["Chat B", "Chat A"])
    #expect(popup?.expired(now: at.addingTimeInterval(60), hovered: false) == false)

    // The owner saw chat B in the app, so its line leaves.
    #expect(prunePopup(popup, inbox: first)?.items.map(\.title) == ["Chat A"])
}
