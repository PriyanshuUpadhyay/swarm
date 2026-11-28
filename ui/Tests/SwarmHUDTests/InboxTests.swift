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
    let cut = String(decoding: data, as: UTF8.self)
        .replacingOccurrences(of: "\"needs_total\":1,\"rows_total\":2", with: "\"needs_total\":4,\"rows_total\":7")
    let more = try #require(InboxSnapshot.read(Data(cut.utf8)))
    #expect(more.needsTotal == 4)
    #expect(wingText(WorkingSnapshot(), best: nil, inbox: InboxSnapshot(rows: more.rows, rowsTotal: more.rowsTotal))
            == WingText(left: "7 done", dot: .done))
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
    // A done chat 30 s later starts a new popup, and the question stays in it.
    let third = InboxRow(id: 3, session: "c", kind: .done, title: "Chat C", body: "Done.")
    let later = mergePopup(popup, new: [PopupItem(row: third)], now: at.addingTimeInterval(30))
    #expect(later?.items.map(\.title) == ["Chat C", "Chat B"])

    // The owner saw chat B in the app, so its line leaves.
    #expect(prunePopup(popup, inbox: first)?.items.map(\.title) == ["Chat A"])
}

/// The HUD closes before an Open is known to work, so a failed Open comes back as a popup line
/// that stays through the next inbox read and takes the place of its chat's older line.
@Test func aFailedOpenStaysAsAPopupLineThroughARead() {
    let at = Date(timeIntervalSince1970: 1_795_000_000)
    let done = InboxRow(id: 1, session: "a", kind: .done, title: "Chat A", body: "Done.")
    let failed = PopupItem(openFailed: "a", agent: nil, title: "Chat A", error: "no pane")
    let popup = mergePopup(mergePopup(nil, new: [PopupItem(row: done)], now: at), new: [failed], now: at)
    #expect(popup?.items.map(\.word) == ["did not open"])
    #expect(popup?.expired(now: at.addingTimeInterval(60), hovered: false) == false)
    #expect(prunePopup(popup, inbox: InboxSnapshot())?.items.map(\.body) == ["no pane"])
}

/// ADR 0156: the inbox lists the chats that still work, and a chat that converges plays the sound
/// once, but not on the HUD's first read or with "Play a sound" off.
@Test func aChatThatConvergesPlaysTheSoundAndAWorkingChatIsActive() throws {
    let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appending(path: "fixtures/inbox.json")
    let inbox = try #require(InboxSnapshot.read(Data(contentsOf: url)))
    #expect(inbox.active.map(\.title) == ["Notice redesign", "Twilio integration review"])

    #expect(!playsSound(inbox, new: newPopupItems(inbox, known: nil)))
    #expect(playsSound(inbox, new: newPopupItems(inbox, known: [])))
    var muted = inbox
    muted.sound = false
    #expect(!playsSound(muted, new: newPopupItems(inbox, known: [])))
}

/// A blocked agent waits for the owner, so its wait pops and plays the sound while Swarm.app is in
/// front; a done or asks chat shows in the app and gets no popup there.
@Test func aWaitPopsAndPlaysTheSoundWhileSwarmIsInFront() throws {
    let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appending(path: "fixtures/inbox.json")
    let inbox = try #require(InboxSnapshot.read(Data(contentsOf: url)))
    let new = newPopupItems(inbox, known: [])
    #expect(popupItems(new, swarmInFront: false).map(\.word) == ["needs you", "asks", "done"])
    #expect(popupItems(new, swarmInFront: true).map(\.word) == ["needs you"])
    #expect(playsSound(inbox, new: popupItems(new, swarmInFront: true)))
}
