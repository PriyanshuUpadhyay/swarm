import Foundation
import Testing
@testable import SwarmCore

@Suite("Skills page model")
@MainActor
struct SkillsPageModelTests {
    @Test("The list keeps every inventory row, including read-only entries and repeated names")
    func listStates() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source)
        await model.loadList()
        #expect(model.entries.count == 3)
        #expect(model.entries.filter { $0.name == "research" }.allSatisfy { $0.qualifier != nil })
        #expect(model.entries.first { $0.name == "plain" }?.hasStepTable == false)
        await source.failList()
        await model.loadList()
        #expect(model.listError != nil)
        #expect(model.entries.count == 3)
    }

    @Test("The first node is selected and rename and move keep its draft identity")
    func selection() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source, checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        #expect(model.selectedID == "01-question")
        let selected = try #require(model.selectedID)
        model.setName("question-edited")
        model.moveSelected(by: 1)
        #expect(model.selectedID == selected)
        #expect(model.selectedStep?.stem == "02-question-edited")
        #expect(model.isDirty)
        #expect(model.canSave)
    }

    @Test("Read-only and script-owned sources disable the correct controls; a cycle choice is disabled")
    func enabledControls() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source, checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        #expect(model.canEdit)
        #expect(model.canChangeStructure)
        #expect(!model.canChooseNeed("02-local"))
        #expect(model.needChoices.allSatisfy { $0.id != model.selectedID })
        let locked = SkillDocument.parse(data: try SkillDocumentTests.fixture("flow-like"), key: fixture.key)
        await source.use(locked)
        await model.reload()
        #expect(model.canEdit)
        #expect(!model.canChangeStructure)
        #expect(model.structuralReason == "this skill's script owns its step ids")
        #expect(model.selectedStep?.body == nil)
        let bundled = SkillDocument.parse(data: fixture.document.bytes, key: fixture.key,
                                         state: .bundledReadOnly(reason: "Set checkout"))
        await source.use(bundled)
        await model.reload()
        #expect(!model.canEdit)
        #expect(!model.canSave)
    }

    @Test("Invalid field text stays visible and disables Save until it is fixed")
    func invalidInput() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let model = SkillsPageModel(source: SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document),
                                    checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.setName("question-")
        #expect(model.nameText == "question-")
        #expect(model.isDirty)
        #expect(!model.canSave)
        #expect(model.error != nil)
        model.select("02-local")
        model.select("01-question")
        #expect(model.nameText == "question-")
        model.setName("question-edited")
        #expect(model.canSave)
        #expect(model.error == nil)
    }

    @Test("Save reloads the bytes, keeps selection and clears the changed state")
    func saveTransition() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source, checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.select("02-local")
        model.setHolds("changed in fixture")
        #expect(await model.save())
        #expect(model.selectedID == "02-local")
        #expect(model.selectedStep?.holds == "changed in fixture")
        #expect(!model.isDirty)
        #expect(model.notice == "Saved in checkout")
        #expect(!model.canSave)
    }

    @Test("Conflict preserves the draft, blocks Save and asks before Reload discards edits")
    func conflictTransition() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source, checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.setHolds("unsaved draft")
        await source.failSave(.conflict(path: "/fixture/checkout/SKILL.md"))
        #expect(await model.save() == false)
        #expect(model.selectedStep?.holds == "unsaved draft")
        #expect(model.conflict)
        #expect(model.error == "This skill changed on disk. Reload before saving.")
        #expect(!model.canSave)
        await model.reload()
        #expect(model.pendingAction == .reload)
        #expect(model.isDirty)
        #expect(await model.resolveNavigation(.cancel) == false)
        #expect(model.isDirty)
        await model.reload()
        _ = await model.resolveNavigation(.discard)
        #expect(!model.isDirty)
        #expect(!model.conflict)
    }

    @Test("A failed Reload keeps all edits, and Discard keeps a known conflict until Reload succeeds")
    func reloadFailure() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source, checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.setHolds("unsaved draft")
        await source.failSave(.conflict(path: "/fixture/SKILL.md"))
        #expect(await model.save() == false)
        await source.failLoad()
        await model.reload()
        #expect(await model.resolveNavigation(.discard) == false)
        #expect(model.isDirty)
        #expect(model.selectedStep?.holds == "unsaved draft")
        #expect(model.canRetry)
        model.discard()
        #expect(!model.isDirty)
        #expect(model.conflict)
        #expect(!model.canSave)
    }

    @Test("Back, checkout changes and close requests all protect a dirty draft")
    func dirtyNavigation() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let model = SkillsPageModel(source: SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document),
                                    checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.setHolds("unsaved draft")
        await model.back()
        #expect(model.pendingAction == .list)
        _ = await model.resolveNavigation(.cancel)
        await model.changeCheckout("/different/checkout", loadError: nil)
        #expect(model.pendingAction == .checkout("/different/checkout"))
        _ = await model.resolveNavigation(.cancel)
        #expect(model.checkoutPath == "/fixture/checkout")
        await model.requestLeave()
        #expect(model.pendingAction == .leave)
        _ = await model.resolveNavigation(.cancel)
        #expect(model.isDirty)
        #expect(model.selectedStep?.holds == "unsaved draft")
    }

    @Test("Discard asks only for a changed draft, and Cancel preserves the edits")
    func discardConfirmation() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let model = SkillsPageModel(source: SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document),
                                    checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        await model.requestDiscard()
        #expect(model.pendingAction == nil)
        model.setHolds("unsaved draft")
        await model.requestDiscard()
        #expect(model.pendingAction == .discard)
        #expect(await model.resolveNavigation(.cancel) == false)
        #expect(model.selectedStep?.holds == "unsaved draft")
        await model.requestDiscard()
        #expect(await model.resolveNavigation(.discard))
        #expect(!model.isDirty)
        #expect(model.pendingAction == nil)
    }

    @Test("A render mismatch keeps the draft and has no Retry action")
    func renderFailureHasNoRetry() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let original = fixture.document
        let malformed = SkillDocument(key: original.key, bytes: original.bytes, sourceURL: original.sourceURL,
                                      revision: original.revision, lineEnding: original.lineEnding,
                                      tableRange: original.tableRange, rows: original.rows, sections: [:],
                                      state: original.state, capability: original.capability)
        let model = SkillsPageModel(source: SkillsPageSourceFixture(inventory: fixture.inventory, document: malformed),
                                    checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.setHolds("changed in fixture")
        #expect(await model.save() == false)
        #expect(model.error == SkillDraftIssue.renderMismatch.localizedDescription)
        #expect(model.isDirty)
        #expect(!model.canRetry)
    }

    @Test("IO failure keeps the draft; Retry saves it, and failed Save cancels a skill change")
    func retryAndNavigation() async throws {
        let fixture = try SkillsPageFixture()
        defer { fixture.clean() }
        let source = SkillsPageSourceFixture(inventory: fixture.inventory, document: fixture.document)
        let model = SkillsPageModel(source: source, checkoutPath: "/fixture/checkout")
        await model.loadList()
        await model.open(key: fixture.key)
        model.setHolds("draft")
        await source.failSave(.io(path: "/fixture/SKILL.md", reason: "fixture failure"))
        await model.open(key: "kit/skills/plain/SKILL.md")
        #expect(model.pendingAction == .open("kit/skills/plain/SKILL.md"))
        #expect(await model.resolveNavigation(.save) == false)
        #expect(model.selectedKey == fixture.key)
        #expect(model.isDirty)
        #expect(model.canRetry)
        await source.failSave(nil)
        await model.retry()
        #expect(!model.isDirty)
        #expect(model.notice == "Saved in checkout")
    }
}

private struct SkillsPageFixture {
    let root: URL
    let key = "kit/skills/research/SKILL.md"
    let inventory: SkillInventory
    let document: SkillDocument

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let bytes = try SkillDocumentTests.fixture("research-like")
        let files: [(String, Data)] = [("kit/skills/research/SKILL.md", bytes),
                                     ("vendor/research/SKILL.md", bytes),
                                     ("kit/skills/plain/SKILL.md", Data("# Plain\n".utf8))]
        for (path, data) in files {
            let target = root.appendingPathComponent(path)
            try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: target)
        }
        let manifest = try JSONSerialization.data(withJSONObject: ["files": files.map { ["path": $0.0] }])
        inventory = try SkillInventory(manifest: manifest, root: root)
        document = SkillDocument.parse(data: bytes, key: key, sourceURL: URL(fileURLWithPath: "/fixture/checkout/SKILL.md"))
    }

    func clean() { try? FileManager.default.removeItem(at: root) }
}

private actor SkillsPageSourceFixture: SkillsPageSource {
    let values: SkillInventory
    var document: SkillDocument
    var saveError: SkillSaveError?
    var listFails = false
    var loadFails = false

    init(inventory: SkillInventory, document: SkillDocument) { values = inventory; self.document = document }
    func inventory() async throws -> SkillInventory {
        if listFails { throw SkillSaveError.io(path: "/fixture/manifest.json", reason: "fixture failure") }
        return values
    }
    func load(key: String, checkoutPath: String?, inventory: SkillInventory) async throws -> SkillDocument {
        if loadFails { throw SkillSaveError.io(path: "/fixture/SKILL.md", reason: "fixture load failure") }
        return document
    }
    func save(document: SkillDocument, candidate: Data, checkoutPath: String, inventory: SkillInventory) async throws -> SkillDocument {
        if let saveError { throw saveError }
        self.document = SkillDocument.parse(data: candidate, key: document.key, sourceURL: document.sourceURL)
        return self.document
    }
    func use(_ document: SkillDocument) { self.document = document }
    func failSave(_ error: SkillSaveError?) { saveError = error }
    func failList() { listFails = true }
    func failLoad() { loadFails = true }
}

@Suite("Skills file source")
@MainActor
struct SkillsFileSourceTests {
    @Test("The real source reads the bundle, falls back on missing checkout files and saves only the checkout")
    func actualSource() async throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let manifest = try JSONSerialization.data(withJSONObject: ["files": [["path": fixture.key]]])
        try manifest.write(to: fixture.bundle.appendingPathComponent("manifest.json"))
        let bundledBytes = try Data(contentsOf: fixture.bundle.appendingPathComponent(fixture.key))
        let source = SkillsFileSource(root: fixture.bundle, protectedRoots: [fixture.bundle])
        let inventory = try await source.inventory()
        let preview = try await source.load(key: fixture.key, checkoutPath: nil, inventory: inventory)
        #expect(preview.bytes == bundledBytes)
        #expect(preview.state != .checkoutEditable)
        let path = fixture.root.appendingPathComponent("checkout").path
        let model = SkillsPageModel(source: source, checkoutPath: path)
        await model.loadList()
        await model.open(key: fixture.key)
        #expect(model.canEdit)
        model.setHolds("saved by the page model")
        let candidate = try #require(try model.draft?.render())
        #expect(await model.save())
        #expect(try Data(contentsOf: fixture.target) == candidate)
        #expect(try Data(contentsOf: fixture.bundle.appendingPathComponent(fixture.key)) == bundledBytes)
        model.setHolds("preserved draft")
        let external = Data("# changed outside the page\n".utf8)
        try external.write(to: fixture.target)
        #expect(await model.save() == false)
        #expect(model.conflict)
        #expect(model.selectedStep?.holds == "preserved draft")
        #expect(try Data(contentsOf: fixture.target) == external)
        try FileManager.default.removeItem(at: fixture.target)
        let fallback = try await source.load(key: fixture.key, checkoutPath: path, inventory: inventory)
        if case .missingCheckoutSource(let reason) = fallback.state { #expect(!reason.isEmpty) }
        else { Issue.record("Missing checkout source must keep a read-only bundle preview") }
        #expect(fallback.bytes == bundledBytes)
        #expect(!FileManager.default.fileExists(atPath: fixture.target.path))
    }
}
