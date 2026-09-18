import Testing
import Foundation
@testable import SwarmCore

/// Saved model presets: applying one writes every field it names, matching tells a preset from a
/// one-off, and the default one is what a new session starts on.
@Suite("Model presets", .scratchDirectory)
struct ModelPresetTests {
    static let opusHigh = ModelPreset(
        name: "Opus 5 High", model: "opus", effort: "high", backend: .claudeCode,
        outputStyle: "Explanatory", permissionMode: .bypassPermissions
    )
    static let solLow = ModelPreset(
        name: "Sol Low", model: "gpt-5.6-sol", effort: "low", backend: .codex,
        permissionMode: .autoReview
    )

    // MARK: - Applying

    /// The bug presets exist for: switching model left the other fields where the last model put
    /// them. Every field is written, and the mode lands on something the new backend has.
    @Test func applyingWritesEveryFieldAtOnce() {
        let before = ComposerControls(
            model: "gpt-5.5", effort: "xhigh", agentKind: .codex,
            permissionMode: .autoReview, outputStyle: "Learning"
        )
        let after = before.applying(Self.opusHigh)
        #expect(after.agentKind == .claudeCode)
        #expect(after.model == "opus")
        #expect(after.effort == "high")
        #expect(after.outputStyle == "Explanatory")
        #expect(after.permissionMode == .bypassPermissions)
        #expect(after.matches(Self.opusHigh))
    }

    @Test func applyingLeavesFastModeAndWorktreeAlone() {
        let before = ComposerControls(isFastMode: true, hasWorktree: false, codexFastMode: true)
        let after = before.applying(Self.solLow)
        #expect(after.isFastMode)
        #expect(after.codexFastMode == true)
        #expect(!after.hasWorktree)
        #expect(after.permissionMode == .autoReview)
    }

    @Test func aPresetCannotStoreAModeItsBackendLacks() {
        let preset = ModelPreset(
            name: "Plan on Codex", model: "gpt-5.5", effort: "low", backend: .codex, permissionMode: .plan
        )
        #expect(preset.permissionMode != .plan)
    }

    // MARK: - Matching

    @Test func anyDifferingFieldIsAOneOff() {
        let controls = ComposerControls().applying(Self.opusHigh)
        var effort = controls
        effort.effort = "medium"
        var style = controls
        style.outputStyle = OutputStyle.defaultName
        var mode = controls
        mode.permissionMode = .acceptEdits
        #expect(controls.matches(Self.opusHigh))
        #expect(!effort.matches(Self.opusHigh))
        #expect(!style.matches(Self.opusHigh))
        #expect(!mode.matches(Self.opusHigh))
    }

    /// Codex has no output styles, so a style the chat carries cannot make it a one-off there.
    @Test func codexIgnoresTheOutputStyle() {
        var controls = ComposerControls().applying(Self.solLow)
        controls.outputStyle = "Learning"
        #expect(controls.matches(Self.solLow))
    }

    @Test func theFirstMatchInMenuOrderWins() {
        var twin = Self.opusHigh
        twin.id = .new()
        twin.name = "Twin"
        let list = ModelPresetList(presets: [Self.solLow, Self.opusHigh, twin])
        let controls = ComposerControls().applying(twin)
        #expect(list.matching(controls)?.name == "Opus 5 High")
        #expect(ModelPresetList(presets: [Self.solLow]).matching(controls) == nil)
    }

    // MARK: - The list

    @Test func deletingTheDefaultClearsItRatherThanPromoting() {
        var list = ModelPresetList(presets: [Self.opusHigh, Self.solLow])
        list.setDefault(Self.opusHigh.id)
        #expect(list.defaultPreset == Self.opusHigh)
        list.delete(id: Self.opusHigh.id)
        #expect(list.defaultID == nil)
        #expect(list.presets == [Self.solLow])
    }

    @Test func aDefaultMustBeInTheList() {
        var list = ModelPresetList(presets: [Self.solLow])
        list.setDefault(Self.opusHigh.id)
        #expect(list.defaultID == nil)
        #expect(ModelPresetList(presets: [Self.solLow], defaultID: Self.opusHigh.id).defaultID == nil)
    }

    @Test func renameTrimsAndRefusesEmpty() {
        var list = ModelPresetList(presets: [Self.opusHigh])
        list.rename(id: Self.opusHigh.id, to: "  Deep  ")
        #expect(list.presets.first?.name == "Deep")
        list.rename(id: Self.opusHigh.id, to: "   ")
        #expect(list.presets.first?.name == "Deep")
    }

    @Test func moving() {
        let third = ModelPreset(name: "Third", model: "sonnet", effort: "low", backend: .claudeCode, permissionMode: .plan)
        var list = ModelPresetList(presets: [Self.opusHigh, Self.solLow, third])
        list.move(fromOffsets: IndexSet(integer: 0), toOffset: 3)
        #expect(list.presets.map(\.name) == ["Sol Low", "Third", "Opus 5 High"])
        list.move(fromOffsets: IndexSet(integer: 2), toOffset: 0)
        #expect(list.presets.map(\.name) == ["Opus 5 High", "Sol Low", "Third"])
        list.move(id: third.id, by: -1)
        #expect(list.presets.map(\.name) == ["Opus 5 High", "Third", "Sol Low"])
        list.move(id: Self.opusHigh.id, by: -1)
        #expect(list.presets.map(\.name) == ["Opus 5 High", "Third", "Sol Low"])
    }

    @Test func anUnreadableValueIsAnEmptyList() {
        #expect(ModelPresetList.decode(nil) == ModelPresetList())
        #expect(ModelPresetList.decode("not json") == ModelPresetList())
        #expect(ModelPresetList().encoded() == nil)
    }

    // MARK: - Fast mode

    @Test func fastModeIsOnlyOfferedWhereItDoesSomething() {
        let claude = ComposerControls()
        let codex = ComposerControls(agentKind: .codex)
        let grok = ComposerControls(agentKind: .grok)
        #expect(claude.fastModeAvailability(codexSpeed: nil, codexSpeedFailed: false) == .available)
        #expect(codex.fastModeAvailability(codexSpeed: nil, codexSpeedFailed: false) == .loading)
        #expect(codex.fastModeAvailability(codexSpeed: nil, codexSpeedFailed: true) == .unavailable)
        #expect(grok.fastModeAvailability(codexSpeed: nil, codexSpeedFailed: false) == .unavailable)
    }

    @Test func fastModeIsWrittenToTheSwitchTheBackendReads() {
        let claude = ComposerControls().settingFastMode(true)
        #expect(claude.isFastMode)
        #expect(claude.codexFastMode == nil)
        let codex = ComposerControls(agentKind: .codex).settingFastMode(false)
        #expect(!codex.isFastMode)
        #expect(codex.codexFastMode == false)
    }

    // MARK: - New sessions

    @Test func theDefaultPresetIsWhatANewSessionStartsOn() async throws {
        let store = try makeTestStore("model-presets-default")
        await AppDefaults(model: "sonnet", effort: "low", permissionMode: .acceptEdits).save(to: store)

        var list = ModelPresetList(presets: [Self.solLow, Self.opusHigh])
        list.setDefault(Self.solLow.id)
        try await list.save(to: store)
        #expect(await ModelPresetList.load(from: store) == list)

        // Settings keeps showing its own values.
        #expect(await AppDefaults.load(from: store).model == "sonnet")

        let defaults = await AppDefaults.loadForNewSessions(from: store)
        let resolved = ComposerDefaults.resolve(repo: RepoSettings(), app: defaults)
        #expect(resolved.model == "gpt-5.6-sol")
        #expect(resolved.backend == .codex)
        #expect(resolved.effort == "low")
        #expect(resolved.permissionMode == .autoReview)
    }

    @Test func withNoDefaultPresetNewSessionsUseSettings() async throws {
        let store = try makeTestStore("model-presets-no-default")
        await AppDefaults(model: "sonnet", effort: "low").save(to: store)
        try await ModelPresetList(presets: [Self.opusHigh]).save(to: store)
        #expect(await AppDefaults.loadForNewSessions(from: store).model == "sonnet")
    }
}
