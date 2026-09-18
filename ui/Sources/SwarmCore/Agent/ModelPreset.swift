import Foundation

/// A named set of the choices the composer's settings panel makes, saved so it can be picked again
/// in one press: "Opus 5 High" rather than a model, a level, a style and a mode chosen one by one.
///
/// **It holds every field it names, and applying it writes every one of them.** Picking a model on
/// its own moves the backend and settles the effort, and leaves the output style and the
/// permission mode wherever the last model left them, which is how a chat came to be on a model it
/// was never meant to run with a mode somebody picked for another one. A preset says all five at
/// once, so nothing is left behind from the choice before.
///
/// Fast mode is deliberately not in here. It is a toggle beside the model button that combines
/// with any preset, and on Codex it depends on a speed the server reports per model, so a preset
/// saying "fast" would be a promise the next model could not keep.
public struct ModelPreset: Identifiable, Codable, Equatable, Sendable {
    public var id: ModelPresetID
    public var name: String
    public var model: String
    public var effort: String
    /// Which CLI the model belongs to. Stored beside the model for the reason `AppDefaults.backend`
    /// is: Codex's list is fetched, and a preset must apply the same way offline.
    public var backend: AgentKind
    public var outputStyle: String
    public var permissionMode: PermissionMode

    public init(
        id: ModelPresetID = .new(),
        name: String,
        model: String,
        effort: String,
        backend: AgentKind,
        outputStyle: String = OutputStyle.defaultName,
        permissionMode: PermissionMode
    ) {
        self.id = id
        self.name = name
        self.model = model
        self.effort = effort
        self.backend = backend
        self.outputStyle = outputStyle
        // Through the same rule `ComposerControls` holds, so a preset can never store a mode its
        // backend has no row for.
        self.permissionMode = permissionMode.nearest(on: backend)
    }

    /// A preset of exactly what the composer is set to now, which is what "Save as preset…" saves.
    public init(name: String, controls: ComposerControls) {
        self.init(
            name: name,
            model: controls.model,
            effort: controls.effort,
            backend: controls.agentKind,
            outputStyle: controls.outputStyle,
            permissionMode: controls.permissionMode
        )
    }

    /// The name with the surrounding space taken off, or nil when nothing is left.
    public static func cleanName(_ raw: String) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

public extension ComposerControls {
    /// These controls with every field the preset names written over, in the one order that
    /// cannot land on a mode the backend lacks: the backend first, whose observer moves the mode,
    /// then the mode itself, which is checked against the backend it now sits on.
    ///
    /// Fast mode, the work mode, the context window and whether there is a worktree are left
    /// alone, because a preset does not name them.
    func applying(_ preset: ModelPreset) -> ComposerControls {
        var next = self
        next.agentKind = preset.backend
        next.model = preset.model
        next.effort = preset.effort
        next.outputStyle = preset.outputStyle
        next.permissionMode = preset.permissionMode
        return next
    }

    /// Whether the composer is set to exactly this preset.
    ///
    /// The output style is only compared where the backend has output styles. Codex ignores the
    /// value entirely, so a Codex preset saved with one style and a chat carrying another are the
    /// same choice, and treating them as different would show a one-off where nothing differs.
    func matches(_ preset: ModelPreset) -> Bool {
        guard agentKind == preset.backend,
              model == preset.model,
              effort == preset.effort,
              permissionMode == preset.permissionMode.nearest(on: agentKind)
        else { return false }
        guard offersOutputStyle else { return true }
        return OutputStyle.isDefault(outputStyle)
            ? OutputStyle.isDefault(preset.outputStyle)
            : outputStyle == preset.outputStyle
    }

    /// Whether the fast mode toggle is drawn, and whether it can be pressed.
    ///
    /// Claude Code's fast mode is `--thinking disabled`, which every Claude Code model takes, so it
    /// is always there. Codex's is a service tier the server offers per model, so it is there once
    /// the server has said so, drawn but not pressable while it is being asked, and absent when it
    /// said no or could not be asked. The other backends send nothing for it at all, so a toggle
    /// there would be a control that changes nothing.
    func fastModeAvailability(codexSpeed: CodexSpeed?, codexSpeedFailed: Bool) -> FastModeAvailability {
        switch agentKind {
        case .claudeCode:
            return .available
        case .codex:
            if let codexSpeed { return codexSpeed.supportsFast ? .available : .unavailable }
            return codexSpeedFailed ? .unavailable : .loading
        case .grok, .cursor, .openCode:
            return .unavailable
        }
    }

    /// Whether fast mode is on, read from wherever this backend keeps it.
    func isFast(codexSpeed: CodexSpeed?) -> Bool {
        agentKind == .codex ? codexSpeed?.isFast(override: codexFastMode) ?? false : isFastMode
    }

    /// These controls with fast mode set, on whichever of the two switches this backend reads.
    func settingFastMode(_ value: Bool) -> ComposerControls {
        var next = self
        if agentKind == .codex {
            next.codexFastMode = value
        } else {
            next.isFastMode = value
        }
        return next
    }
}

/// See `ComposerControls.fastModeAvailability`.
public enum FastModeAvailability: Equatable, Sendable {
    case available
    case loading
    case unavailable
}

/// The owner's presets, in the order the menu lists them, and which one new sessions start on.
///
/// One JSON value in the store's key value table rather than a table of its own. It is a short
/// list with one writer, edited as a whole in Settings, and the order is part of the value: a
/// table would need a position column and a write per row to move one.
public struct ModelPresetList: Codable, Equatable, Sendable {
    public static let key = "defaults.modelPresets"

    public private(set) var presets: [ModelPreset]
    /// Nil when new sessions start on the model settings in Settings rather than on a preset,
    /// which is where every copy of Swarm starts.
    public private(set) var defaultID: ModelPresetID?

    public init(presets: [ModelPreset] = [], defaultID: ModelPresetID? = nil) {
        self.presets = presets
        self.defaultID = defaultID.flatMap { id in presets.contains { $0.id == id } ? id : nil }
    }

    public var defaultPreset: ModelPreset? {
        defaultID.flatMap(preset(id:))
    }

    public func preset(id: ModelPresetID) -> ModelPreset? {
        presets.first { $0.id == id }
    }

    /// The first preset the controls match, in menu order. First rather than only, because two
    /// presets may hold the same choices under different names and the menu can tick one.
    public func matching(_ controls: ComposerControls) -> ModelPreset? {
        presets.first { controls.matches($0) }
    }

    public mutating func add(_ preset: ModelPreset) {
        presets.append(preset)
    }

    /// Replaces the preset with the same id, keeping its place in the list.
    public mutating func update(_ preset: ModelPreset) {
        guard let index = presets.firstIndex(where: { $0.id == preset.id }) else { return }
        presets[index] = preset
    }

    public mutating func rename(id: ModelPresetID, to name: String) {
        guard let name = ModelPreset.cleanName(name),
              let index = presets.firstIndex(where: { $0.id == id })
        else { return }
        presets[index].name = name
    }

    /// Deleting the default leaves new sessions on the Settings defaults rather than promoting
    /// whichever preset happens to be next, which would be a choice nobody made.
    public mutating func delete(id: ModelPresetID) {
        presets.removeAll { $0.id == id }
        if defaultID == id { defaultID = nil }
    }

    /// Nil clears the default. An id that is not in the list does nothing.
    public mutating func setDefault(_ id: ModelPresetID?) {
        guard let id else {
            defaultID = nil
            return
        }
        guard preset(id: id) != nil else { return }
        defaultID = id
    }

    public mutating func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        let moving = source.sorted().filter { presets.indices.contains($0) }
        guard !moving.isEmpty else { return }
        let items = moving.map { presets[$0] }
        let before = moving.filter { $0 < destination }.count
        for index in moving.reversed() { presets.remove(at: index) }
        let target = min(max(destination - before, 0), presets.count)
        presets.insert(contentsOf: items, at: target)
    }

    /// Moves one preset a single place up (negative) or down (positive), clamped to the list.
    public mutating func move(id: ModelPresetID, by offset: Int) {
        guard let index = presets.firstIndex(where: { $0.id == id }) else { return }
        let target = min(max(index + offset, 0), presets.count - 1)
        guard target != index else { return }
        let preset = presets.remove(at: index)
        presets.insert(preset, at: target)
    }

    // MARK: - Storage

    /// A missing or unreadable value is an empty list. Presets are a convenience, and a row that
    /// no longer decodes must not stop a chat from opening.
    public static func decode(_ raw: String?) -> ModelPresetList {
        guard let raw, let data = raw.data(using: .utf8),
              let list = try? JSONDecoder().decode(ModelPresetList.self, from: data)
        else { return ModelPresetList() }
        return ModelPresetList(presets: list.presets, defaultID: list.defaultID)
    }

    public func encoded() -> String? {
        guard !presets.isEmpty, let data = try? JSONEncoder().encode(self) else { return nil }
        return String(decoding: data, as: UTF8.self)
    }

    public static func load(from store: Store) async -> ModelPresetList {
        decode(try? await store.setting(key))
    }

    public func save(to store: Store) async throws {
        try await store.setSetting(Self.key, encoded())
    }
}

public extension AppDefaults {
    /// These defaults with the default preset's choices written over the model, the effort, the
    /// backend, the output style and the permission mode.
    ///
    /// The stored model and effort move too, because those are what `ComposerDefaults.resolve`
    /// asks to decide whether the owner chose anything, and a default preset is a choice. A
    /// repository's own settings file still outranks it, exactly as it outranks the Models screen.
    /// "Start in plan mode" still wins over the mode, for the reason `resolve` gives.
    func applying(_ preset: ModelPreset) -> AppDefaults {
        var next = self
        next.model = preset.model
        next.storedModel = preset.model
        next.effort = preset.effort
        next.storedEffort = preset.effort
        next.backend = preset.backend
        next.outputStyle = preset.outputStyle
        next.permissionMode = preset.permissionMode
        return next
    }

    /// What a brand new session inherits: the Settings defaults, with the default preset over
    /// them when there is one. Settings itself reads `load`, so the screen keeps showing and
    /// saving its own values rather than the preset's.
    static func loadForNewSessions(from store: Store) async -> AppDefaults {
        let defaults = await load(from: store)
        guard let preset = await ModelPresetList.load(from: store).defaultPreset else { return defaults }
        return defaults.applying(preset)
    }
}
