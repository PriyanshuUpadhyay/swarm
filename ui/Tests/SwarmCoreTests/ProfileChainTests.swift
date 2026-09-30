import Foundation
import Testing
@testable import SwarmCore

@Suite("Profile chain")
struct ProfileChainTests {
    private let lowUsage = SwarmSkip(index: 0, code: "low_usage", text: "usage 2% left (threshold 5%)")
    private let signedOut = SwarmSkip(index: 1, code: "signed_out", text: "no codex account is signed in")

    @Test("A healthy chain has no marks; a fallback marks the skipped runner and the one that runs")
    func chipStates() {
        #expect(RunnerChipState.states(runners: 2, check: nil) == [.normal, .normal])
        #expect(RunnerChipState.states(runners: 2, check: SwarmProfileCheck(name: "chat", pick: 0, skipped: []))
            == [.normal, .normal])
        #expect(RunnerChipState.states(runners: 3, check: SwarmProfileCheck(name: "chat", pick: 1, skipped: [lowUsage]))
            == [.skipped(short: "usage 2% left", full: "usage 2% left (threshold 5%)"), .next, .normal])
        #expect(RunnerChipState.states(runners: 2, check: SwarmProfileCheck(name: "chat", pick: nil, skipped: [lowUsage, signedOut]))
            == [.skipped(short: "usage 2% left", full: "usage 2% left (threshold 5%)"),
                .skipped(short: "signed out", full: "no codex account is signed in")])
    }

    @Test("A cut chain drops chips from the end but keeps the one the next launch takes")
    func fit() {
        #expect(ChainFit(count: 6, pick: nil, limit: 3) == ChainFit(count: 6, pick: 0, limit: 3))
        let fromStart = ChainFit(count: 6, pick: 1, limit: 3)
        #expect(fromStart.shown == [0, 1, 2] && !fromStart.leadingCut && fromStart.hidden == 3)
        let late = ChainFit(count: 6, pick: 4, limit: 2)
        #expect(late.shown == [3, 4] && late.leadingCut && late.hidden == 4)
        let all = ChainFit(count: 2, pick: 1, limit: 9)
        #expect(all.shown == [0, 1] && !all.leadingCut && all.hidden == 0)
    }

    @Test("Groups take the name before the first dot, chat first, then by name")
    func groups() {
        let names = ["search.web", "code.small", "chat", "code.routine", "solo"]
        let groups = ProfileGroup.groups(names.map { SwarmProfile(name: $0, runners: []) })
        #expect(groups.map(\.name) == ["chat", "code", "search", "solo"])
        #expect(groups[1].profiles.map(\.name) == ["code.small", "code.routine"])
    }

    @Test("Health counts ready, fallback, and blocked; an unknown status counts nowhere")
    func health() {
        let health = ProfileHealth([
            ProfileStatus(kind: .primary, text: ""), ProfileStatus(kind: .fallback, text: ""),
            ProfileStatus(kind: .none, text: ""), nil,
        ])
        #expect(health == { var h = ProfileHealth([]); h.ready = 1; h.fallback = 1; h.blocked = 1; return h }())
    }

    @Test("A card shows the check only while the draft matches the saved profile")
    func cardStatus() {
        let profile = SwarmProfile(name: "code.complex", runners: [
            SwarmRunner(provider: "claude", model: "opus", effort: "high"),
            SwarmRunner(provider: "codex", model: "gpt-6.1-sol", effort: "high"),
            SwarmRunner(provider: "agy", model: "flash", effort: "medium"),
        ])
        let check = SwarmProfileCheck(name: "code.complex", pick: 1, skipped: [lowUsage])
        var draft = ProfileDraft(profile)
        #expect((0..<3).map { draft.cardStatus(at: $0, check: check) }
            == [.skipped("usage 2% left (threshold 5%)"), .next, .standby])
        #expect(draft.cardStatus(at: 0, check: nil) == .unknown)
        draft.runners[1].effort = "xhigh"
        draft.runners.append(SwarmRunner(provider: "claude", model: "sonnet", effort: "low"))
        #expect((0..<4).map { draft.cardStatus(at: $0, check: check) }
            == [.saveToUpdate, .changed, .saveToUpdate, .added])
    }
}
