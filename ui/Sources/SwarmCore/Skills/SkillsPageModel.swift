import Foundation
import Observation

public protocol SkillsPageSource: Sendable {
    func inventory() async throws -> SkillInventory
    func load(key: String, checkoutPath: String?, inventory: SkillInventory) async throws -> SkillDocument
    func save(document: SkillDocument, candidate: Data, checkoutPath: String, inventory: SkillInventory) async throws -> SkillDocument
}

public enum SkillsNavigationAction: Equatable, Sendable {
    case open(String), list, reload, discard, checkout(String?), leave
}

public enum SkillsNavigationChoice: Sendable { case save, discard, cancel }

@MainActor @Observable
public final class SkillsPageModel {
    public private(set) var entries: [SkillInventoryEntry] = []
    public private(set) var listError: String?
    public private(set) var loadingList = false
    public private(set) var busy = false
    public private(set) var selectedKey: String?
    public private(set) var draft: SkillDraft?
    public private(set) var selectedID: String?
    public private(set) var checkoutPath: String?
    public private(set) var settingsError: String?
    public private(set) var notice: String?
    public private(set) var conflict = false
    public private(set) var pendingAction: SkillsNavigationAction?
    public private(set) var announcementRevision = 0
    private var operationError: String?
    private var nameInputs: [String: String] = [:]
    private var holdsInputs: [String: String] = [:]
    private var bodyInputs: [String: String] = [:]
    private var fieldErrors: [String: String] = [:]
    private var retrySave = false
    private var retryKey: String?
    private var pendingCheckout: SkillsNavigationAction?
    private var allowedNeeds: Set<String> = []
    @ObservationIgnored private let source: any SkillsPageSource
    @ObservationIgnored private var inventory: SkillInventory?

    public init(source: any SkillsPageSource, checkoutPath: String? = nil, settingsError: String? = nil) {
        self.source = source
        self.checkoutPath = checkoutPath
        self.settingsError = settingsError
    }

    public var checkoutChangePending: Bool { pendingCheckout != nil }
    public var document: SkillDocument? { draft?.document }
    public var steps: [SkillDraftStep] { draft?.steps ?? [] }
    public var selectedStep: SkillDraftStep? { steps.first { $0.id == selectedID } }
    public var needChoices: [SkillDraftStep] { steps.filter { $0.id != selectedID } }
    public var isDirty: Bool { draft?.isChanged == true || !fieldErrors.isEmpty }
    public var canEdit: Bool { document?.state == .checkoutEditable && checkoutPath != nil && settingsError == nil && !busy }
    public var canChangeStructure: Bool { canEdit && document?.capability == .generic }
    public var canAdd: Bool { canChangeStructure && fieldErrors.isEmpty && steps.count < 99 }
    public var structuralReason: String? { document?.capability.reason }
    public var canSave: Bool { canEdit && isDirty && fieldErrors.isEmpty && draft?.validate().isEmpty == true && !conflict }
    public var canRetry: Bool { !busy && (retryKey != nil || (!conflict && (retrySave || isMissingSource))) }
    public var isMissingSource: Bool { if case .missingCheckoutSource = document?.state { true } else { false } }
    public var nameText: String { selectedID.flatMap { nameInputs[$0] } ?? selectedStep?.name ?? "" }
    public var holdsText: String { selectedID.flatMap { holdsInputs[$0] } ?? selectedStep?.holds ?? "" }
    public var bodyText: String { selectedID.flatMap { bodyInputs[$0] } ?? selectedStep?.body ?? "" }
    public var error: String? {
        operationError ?? fieldErrors.sorted { $0.key < $1.key }.first?.value
            ?? settingsError ?? (document?.state == .checkoutEditable ? draft?.validate().first?.localizedDescription : nil)
    }
    public var removalReason: String? {
        guard let selectedStep else { return nil }
        if let structuralReason { return structuralReason }
        let dependents = steps.filter { $0.needs.contains(selectedStep.id) }.map(\.stem)
        if !dependents.isEmpty { return SkillDraftIssue.dependentSteps(id: selectedStep.stem, dependents: dependents).localizedDescription }
        return steps.count == 1 ? SkillDraftIssue.rowCount.localizedDescription : nil
    }
    public var canRemove: Bool { canChangeStructure && removalReason == nil && fieldErrors.isEmpty }
    public func canMove(by delta: Int) -> Bool {
        guard canChangeStructure, fieldErrors.isEmpty, let index = steps.firstIndex(where: { $0.id == selectedID }) else { return false }
        return steps.indices.contains(index + delta)
    }

    public func loadList() async {
        guard !loadingList else { return }
        loadingList = true
        defer { loadingList = false }
        do {
            let inventory = try await source.inventory()
            self.inventory = inventory
            entries = inventory.entries
            listError = nil
        } catch { listError = error.localizedDescription; announce() }
    }

    public func select(_ id: String) {
        guard !busy, steps.contains(where: { $0.id == id }) else { return }
        selectedID = id
        refreshAllowedNeeds()
    }

    public func setName(_ text: String) {
        guard canChangeStructure, let id = selectedID else { return }
        nameInputs[id] = text
        edit(field: "name:" + id) { try $0.rename(id: id, name: text) }
        if fieldErrors["name:" + id] == nil { nameInputs[id] = nil }
    }

    public func setHolds(_ text: String) {
        guard canEdit, let id = selectedID else { return }
        holdsInputs[id] = text
        edit(field: "holds:" + id) { try $0.setHolds(id: id, holds: text) }
        if fieldErrors["holds:" + id] == nil, selectedStep?.holds == text { holdsInputs[id] = nil }
    }

    public func setBody(_ text: String) {
        guard canEdit, let id = selectedID else { return }
        bodyInputs[id] = text
        edit(field: "body:" + id) { try $0.setBody(id: id, body: text) }
        if fieldErrors["body:" + id] == nil { bodyInputs[id] = nil }
    }

    public func canChooseNeed(_ id: String) -> Bool { canEdit && allowedNeeds.contains(id) }

    private func refreshAllowedNeeds() {
        guard let draft, let selectedStep else { allowedNeeds = []; return }
        allowedNeeds = Set(needChoices.compactMap { step in
            if selectedStep.needs.contains(step.id) { return step.id }
            var proposed = draft
            do {
                try proposed.setNeeds(id: selectedStep.id, needs: selectedStep.needs + [step.id])
                return step.id
            } catch { return nil }
        })
    }

    public func setNeed(_ id: String, selected: Bool) {
        guard canEdit, let step = selectedStep else { return }
        let needs = selected ? step.needs + [id] : step.needs.filter { $0 != id }
        edit { try $0.setNeeds(id: step.id, needs: needs) }
    }

    public func clearNeeds() {
        guard canEdit, let id = selectedID else { return }
        edit { try $0.setNeeds(id: id, needs: []) }
    }

    public func addStep(withSection: Bool) {
        guard canAdd else { return }
        var name = "new-step"
        var suffix = 2
        while steps.contains(where: { $0.name == name }) { name = "new-step-\(suffix)"; suffix += 1 }
        edit { selectedID = try $0.add(name: name, body: withSection ? "" : nil) }
    }

    public func removeSelected(confirmSection: Bool = false) {
        guard canRemove, let id = selectedID, let index = steps.firstIndex(where: { $0.id == id }) else { return }
        edit { try $0.remove(id: id, removeSection: confirmSection) }
        if !steps.contains(where: { $0.id == id }) {
            selectedID = steps[min(index, steps.count - 1)].id
            refreshAllowedNeeds()
        }
    }

    public func moveSelected(by delta: Int) {
        guard canMove(by: delta), let id = selectedID, let index = steps.firstIndex(where: { $0.id == id }) else { return }
        edit { try $0.move(id: id, to: index + delta) }
    }

    public func discard() async {
        await discard(applyingCheckout: true)
    }

    private func discard(applyingCheckout: Bool) async {
        guard !busy, let document else { return }
        let stem = document.rows.first { $0.id == selectedID }?.stem
        let hadConflict = conflict
        install(document, keepingStem: stem)
        if hadConflict {
            conflict = true
            operationError = "This skill changed on disk. Reload before saving."
        }
        if applyingCheckout { _ = await applyPendingCheckout() }
    }

    @discardableResult
    public func save() async -> Bool {
        let saved = await saveDraft()
        _ = await applyPendingCheckout()
        return saved
    }

    private func saveDraft() async -> Bool {
        guard canSave, let draft, let checkoutPath, let inventory else { return false }
        busy = true
        defer { busy = false }
        do {
            guard let candidate = try draft.render() else { return false }
            let stem = selectedStep?.stem
            let saved = try await source.save(document: draft.document, candidate: candidate, checkoutPath: checkoutPath, inventory: inventory)
            install(saved, keepingStem: stem)
            notice = "Saved in checkout"
            announce()
            return true
        } catch let failure as SkillSaveError {
            switch failure {
            case .conflict: conflict = true; operationError = "This skill changed on disk. Reload before saving."
            case .io, .path: operationError = failure.localizedDescription; retrySave = true
            }
        } catch let issue as SkillDraftIssue {
            operationError = issue.localizedDescription
            retrySave = false
        } catch { operationError = error.localizedDescription; retrySave = true }
        announce()
        return false
    }

    public func retry() async {
        if retrySave { _ = await save() }
        else if let key = retryKey ?? selectedKey { _ = await load(key: key, keepingStem: selectedStep?.stem) }
        else { await loadList() }
    }

    public func open(key: String) async { await request(.open(key)) }
    public func back() async { await request(.list) }
    public func reload() async { await request(.reload) }
    public func requestDiscard() async { await request(.discard) }
    public func requestLeave() async { await request(.leave) }
    public func changeCheckout(_ path: String?, loadError: String?) async {
        settingsError = loadError
        guard path != checkoutPath else {
            pendingCheckout = nil
            if case .checkout = pendingAction { pendingAction = nil }
            return
        }
        pendingCheckout = .checkout(path)
        guard !busy else { return }
        if isDirty {
            if pendingAction == nil { pendingAction = pendingCheckout }
        }
        else { _ = await applyPendingCheckout() }
    }

    public func resolveNavigation(_ choice: SkillsNavigationChoice) async -> Bool {
        guard let action = pendingAction else { return false }
        pendingAction = nil
        switch choice {
        case .cancel: return false
        case .save: guard await saveDraft() else { return false }
        case .discard:
            if action == .list || action == .leave || action == .discard || pendingCheckout != nil {
                await discard(applyingCheckout: false)
            }
        }
        if action != .list && action != .leave { _ = await applyPendingCheckout() }
        if case .checkout = action { return pendingCheckout == nil && retryKey == nil }
        return await perform(action)
    }

    private func request(_ action: SkillsNavigationAction) async {
        guard !busy else { return }
        _ = await applyPendingCheckout()
        if isDirty { pendingAction = action }
        else { _ = await perform(action) }
    }

    @discardableResult
    private func applyPendingCheckout() async -> Bool {
        guard !busy, !isDirty, let action = pendingCheckout else { return false }
        pendingCheckout = nil
        if case .checkout = pendingAction { pendingAction = nil }
        return await perform(action)
    }

    private func perform(_ action: SkillsNavigationAction) async -> Bool {
        switch action {
        case .open(let key): return await load(key: key)
        case .reload:
            guard let selectedKey else { return false }
            return await load(key: selectedKey, keepingStem: selectedStep?.stem)
        case .discard: await discard(); return true
        case .checkout(let path):
            checkoutPath = path
            guard let selectedKey else { return true }
            let stem = selectedStep?.stem
            draft = nil; selectedID = nil; allowedNeeds = []
            return await load(key: selectedKey, keepingStem: stem)
        case .list:
            selectedKey = nil; draft = nil; selectedID = nil; allowedNeeds = []; clearErrors(); return true
        case .leave: return true
        }
    }

    private func load(key: String, keepingStem: String? = nil) async -> Bool {
        let loaded = await loadDocument(key: key, keepingStem: keepingStem)
        if pendingCheckout != nil && !isDirty { return await applyPendingCheckout() }
        return loaded
    }

    private func loadDocument(key: String, keepingStem: String?) async -> Bool {
        guard !busy, let inventory else { return false }
        busy = true
        defer { busy = false }
        do {
            let document = try await source.load(key: key, checkoutPath: settingsError == nil ? checkoutPath : nil, inventory: inventory)
            if isDirty, case .missingCheckoutSource(let reason) = document.state {
                operationError = reason
                retryKey = key
                announce()
                return false
            }
            selectedKey = key
            install(document, keepingStem: keepingStem)
            return true
        } catch {
            retryKey = key
            operationError = error.localizedDescription
            announce()
            return false
        }
    }

    private func install(_ document: SkillDocument, keepingStem: String?) {
        draft = SkillDraft(document: document)
        selectedID = steps.first { $0.stem == keepingStem }?.id ?? steps.first?.id
        clearErrors()
        refreshAllowedNeeds()
    }

    private func clearErrors() {
        nameInputs = [:]; holdsInputs = [:]; bodyInputs = [:]; fieldErrors = [:]
        conflict = false; operationError = nil; retrySave = false; retryKey = nil; notice = nil
    }

    private func edit(field: String? = nil, _ change: (inout SkillDraft) throws -> Void) {
        guard var next = draft else { return }
        do {
            try change(&next)
            draft = next
            refreshAllowedNeeds()
            if let field { fieldErrors[field] = nil }
            operationError = nil
            notice = nil
        } catch {
            if let field { fieldErrors[field] = error.localizedDescription }
            else { operationError = error.localizedDescription }
            announce()
        }
    }

    private func announce() { announcementRevision += 1 }
}
