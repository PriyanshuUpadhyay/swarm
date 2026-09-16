import Foundation
import Observation
import BloomCore

/// The owner's model presets as the window draws them, kept in step with the value in the store.
///
/// Shared for the reason `QuickPromptCatalog.shared` gives: the composer is rebuilt every time the
/// centre column changes tab, and the Settings window edits the same list the footer's menu shows,
/// so there has to be exactly one copy for an edit in one to be seen in the other.
///
/// Every edit is applied here first and then written as the whole list, in order. The list has
/// this one writer, so a whole-value write cannot roll back somebody else's.
@MainActor
@Observable
final class ModelPresetLibrary {
    static let shared = ModelPresetLibrary()

    private(set) var list = ModelPresetList()
    private(set) var isLoaded = false

    private var saving: Task<Void, Never>?

    var presets: [ModelPreset] { list.presets }

    func load(from store: Store?) async {
        guard !isLoaded, let store else { return }
        list = await ModelPresetList.load(from: store)
        isLoaded = true
    }

    @discardableResult
    func add(_ preset: ModelPreset, in store: Store?) -> ModelPreset {
        change(in: store) { $0.add(preset) }
        return preset
    }

    func update(_ preset: ModelPreset, in store: Store?) {
        change(in: store) { $0.update(preset) }
    }

    func rename(id: ModelPresetID, to name: String, in store: Store?) {
        change(in: store) { $0.rename(id: id, to: name) }
    }

    func delete(id: ModelPresetID, in store: Store?) {
        change(in: store) { $0.delete(id: id) }
    }

    func setDefault(_ id: ModelPresetID?, in store: Store?) {
        change(in: store) { $0.setDefault(id) }
    }

    func move(fromOffsets source: IndexSet, toOffset destination: Int, in store: Store?) {
        change(in: store) { $0.move(fromOffsets: source, toOffset: destination) }
    }

    func move(id: ModelPresetID, by offset: Int, in store: Store?) {
        change(in: store) { $0.move(id: id, by: offset) }
    }

    /// Applies an edit and writes the result, chained behind the write before it so two quick
    /// edits cannot land in the wrong order.
    private func change(in store: Store?, _ edit: (inout ModelPresetList) -> Void) {
        var next = list
        edit(&next)
        guard next != list else { return }
        list = next
        guard let store else { return }
        let pending = saving
        saving = Task {
            await pending?.value
            try? await next.save(to: store)
        }
    }
}
