import Foundation
import Testing
@testable import SwarmCore

@Suite("Profile editing")
struct ProfileDraftTests {
    private let claude = SwarmProvider(
        id: "claude", label: "Claude", efforts: ["low", "medium", "high"], defaultEffort: "medium",
        accounts: true,
        fields: [SwarmProviderField(name: "permission", label: "Permission", values: ["auto", "plan"], default: "auto")]
    )
    private let codex = SwarmProvider(
        id: "codex", label: "Codex", efforts: ["low", "medium", "high", "ultra"], defaultEffort: "medium",
        accounts: true,
        fields: [SwarmProviderField(name: "sandbox", label: "Sandbox", values: ["read-only", "workspace-write"], default: "workspace-write")]
    )

    private var codeComplex: SwarmProfile {
        SwarmProfile(name: "code.complex", runners: [
            SwarmRunner(provider: "codex", model: "gpt-6.1-sol", effort: "high", sandbox: "workspace-write"),
        ])
    }

    @Test("Add, reorder, and remove make a savable change; the last runner stays")
    func editing() {
        var draft = ProfileDraft(codeComplex)
        #expect(!draft.canSave)
        draft.add(from: [codex, claude]) { $0 == "claude" ? "opus" : nil }
        #expect(draft.runners.last == SwarmRunner(provider: "claude", model: "opus", effort: "medium", permission: "auto"))
        draft.move(fromOffsets: IndexSet(integer: 1), toOffset: 0)
        #expect(draft.runners.map(\.provider) == ["claude", "codex"])
        draft.move(fromOffsets: IndexSet(integer: 0), toOffset: 2)
        #expect(draft.runners.map(\.provider) == ["codex", "claude"])
        draft.move(fromOffsets: IndexSet(integer: 1), toOffset: 0)
        #expect(draft.canSave)
        draft.remove(at: 0)
        #expect(!draft.canSave)
        draft.remove(at: 0)
        #expect(draft.runners.count == 1)
    }

    @Test("A provider change resets model, effort, and flags but keeps the row's identity")
    func providerChange() {
        var draft = ProfileDraft(codeComplex)
        let row = draft.runners[0].id
        draft.setProvider(claude, at: 0, firstModel: "sonnet")
        #expect(draft.runners[0] == SwarmRunner(provider: "claude", model: "sonnet", effort: "medium", permission: "auto"))
        #expect(draft.runners[0].id == row)
    }

    @Test("An empty model cannot be saved")
    func emptyModel() {
        var draft = ProfileDraft(codeComplex)
        draft.runners[0].model = ""
        #expect(!draft.canSave)
    }

    @Test("A model's own efforts win, and the current effort is never dropped")
    func efforts() {
        let runner = SwarmRunner(provider: "codex", model: "gpt-5.5", effort: "ultra")
        let models = [SwarmModel(id: "gpt-5.5", label: "GPT-5.5", efforts: ["low", "xhigh"])]
        #expect(ProfileDraft.efforts(for: runner, provider: codex, models: models) == ["low", "xhigh", "ultra"])
        #expect(ProfileDraft.efforts(for: runner, provider: codex, models: []) == ["low", "medium", "high", "ultra"])
    }

    @Test("Values outside the CLI lists are warned about, not refused")
    func warnings() {
        let runner = SwarmRunner(provider: "codex", model: "gpt-9", effort: "high", sandbox: "danger-full-access")
        let models = [SwarmModel(id: "gpt-6.1-sol", label: "GPT-6.1-Sol")]
        #expect(ProfileDraft.warnings(for: runner, provider: codex, models: models) == [
            "Model \"gpt-9\" is not in Codex's list. It is saved as typed.",
            "Sandbox \"danger-full-access\" is not in the list the CLI showed. It is saved as typed.",
        ])
        #expect(ProfileDraft.warnings(for: runner, provider: codex, models: []).count == 1)
    }

    @Test("The status line names the next runner and each skip")
    func status() {
        let profile = SwarmProfile(name: "review.surface", runners: [
            SwarmRunner(provider: "claude", model: "opus", effort: "high"),
            SwarmRunner(provider: "codex", model: "gpt-6.1-sol", effort: "high"),
        ])
        #expect(ProfileStatus(check: nil, profile: profile) == nil)
        #expect(ProfileStatus(check: SwarmProfileCheck(name: "review.surface", pick: 0, skipped: []), profile: profile)
            == ProfileStatus(kind: .primary, text: "Next launch: claude"))
        let low = SwarmSkip(index: 0, code: "low_usage", text: "usage 2% left (threshold 5%)")
        #expect(ProfileStatus(check: SwarmProfileCheck(name: "review.surface", pick: 1, skipped: [low]), profile: profile)
            == ProfileStatus(kind: .fallback, text: "Next launch: codex. claude skipped: usage 2% left (threshold 5%)"))
        let missing = SwarmSkip(index: 1, code: "cli_missing", text: "codex CLI not found on PATH")
        #expect(ProfileStatus(check: SwarmProfileCheck(name: "review.surface", pick: nil, skipped: [low, missing]), profile: profile)
            == ProfileStatus(kind: .none, text: "No runner can run. claude: usage 2% left (threshold 5%); codex: codex CLI not found on PATH"))
    }

    @Test("New Chat opens on the profile's next runner and marks any other pick as a one-off")
    func chatChoice() throws {
        let chat = SwarmProfile(name: "chat", runners: [
            SwarmRunner(provider: "claude", model: "opus", effort: "high"),
            SwarmRunner(provider: "codex", model: "gpt-6.1-sol", effort: "xhigh"),
        ])
        let fresh = try #require(ChatProfileChoice(profile: chat, check: nil))
        #expect(fresh.runner.provider == "claude")
        #expect(fresh.caption(provider: "claude", model: "opus", defaultEffort: nil)
            == "From the chat profile · high effort · falls back to codex")
        #expect(fresh.caption(provider: "codex", model: "gpt-6-luna", defaultEffort: "medium")
            == "One-off pick · xhigh effort · no fallback")
        #expect(fresh.caption(provider: "agy", model: "flash", defaultEffort: "medium")
            == "One-off pick · medium effort · no fallback")
        let spent = try #require(ChatProfileChoice(
            profile: chat,
            check: SwarmProfileCheck(name: "chat", pick: 1, skipped: [SwarmSkip(index: 0, code: "low_usage", text: "x")])
        ))
        #expect(spent.runner.provider == "codex")
        #expect(spent.caption(provider: "codex", model: "gpt-6.1-sol", defaultEffort: nil)
            == "From the chat profile · xhigh effort · no fallback")
        #expect(ChatProfileChoice(profile: nil, check: nil) == nil)
    }
}
