import Foundation
import Testing
@testable import SwarmCore

struct SkillCheckoutFixture: Sendable {
    let root: URL
    let bundle: URL
    let target: URL
    let key = "kit/skills/research/SKILL.md"
    let inventory: SkillInventory

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        bundle = root.appendingPathComponent("bundle")
        target = root.appendingPathComponent("checkout/skills/kit/skills/research/SKILL.md")
        try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("gitdir: /unused/worktree/git\n".utf8).write(to: root.appendingPathComponent("checkout/.git"))
        try SkillDocumentTests.fixture("research-like").write(to: target)
        let preview = bundle.appendingPathComponent(key)
        try FileManager.default.createDirectory(at: preview.deletingLastPathComponent(), withIntermediateDirectories: true)
        try SkillDocumentTests.fixture("research-like").write(to: preview)
        let manifest = try JSONSerialization.data(withJSONObject: ["files": [["path": key]]])
        inventory = try SkillInventory(manifest: manifest, root: bundle)
    }

    func checkout(protectedRoots: [URL] = []) throws -> SkillCheckout {
        try SkillCheckout(path: root.appendingPathComponent("checkout").path, inventory: inventory, protectedRoots: protectedRoots)
    }

    func clean() { try? FileManager.default.removeItem(at: root) }
}

@Suite("Skill checkout")
struct SkillCheckoutTests {
    @Test("Linked worktrees and root aliases validate to the canonical directory")
    func paths() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let alias = fixture.root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.root.appendingPathComponent("checkout"))
        #expect(try SkillCheckout.validate(path: alias.path, protectedRoots: []) == fixture.root.appendingPathComponent("checkout").resolvingSymlinksInPath().path)
        for path in ["", "relative/path", fixture.bundle.path] {
            #expect(throws: SkillSaveError.self) { try SkillCheckout.validate(path: path, protectedRoots: []) }
        }
        try FileManager.default.removeItem(at: fixture.root.appendingPathComponent("checkout/.git"))
        try FileManager.default.createDirectory(at: fixture.root.appendingPathComponent("checkout/.git"), withIntermediateDirectories: false)
        #expect(try fixture.checkout().load(key: fixture.key).rows.count == 6)
    }

    @Test("Load and save use actual bytes and retain file mode; no-op keeps the inode")
    func save() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        try FileManager.default.setAttributes([.posixPermissions: 0o640], ofItemAtPath: fixture.target.path)
        let checkout = try fixture.checkout()
        let loaded = try checkout.load(key: fixture.key)
        #expect(loaded.sourceURL == fixture.target.resolvingSymlinksInPath())
        let attributes = try FileManager.default.attributesOfItem(atPath: fixture.target.path)
        let before = try #require(attributes[.systemFileNumber] as? NSNumber)
        #expect(try checkout.save(key: fixture.key, candidate: loaded.bytes, expectedRevision: loaded.revision) == loaded.revision)
        #expect(try FileManager.default.attributesOfItem(atPath: fixture.target.path)[.systemFileNumber] as? NSNumber == before)
        var draft = SkillDraft(document: loaded)
        try draft.setHolds(id: "01-question", holds: "saved in fixture")
        let candidate = try #require(try draft.render())
        let revision = try checkout.save(key: fixture.key, candidate: candidate, expectedRevision: loaded.revision)
        #expect(try Data(contentsOf: fixture.target) == candidate)
        #expect(revision == SkillDocument.revision(of: candidate))
        #expect(try FileManager.default.attributesOfItem(atPath: fixture.target.path)[.posixPermissions] as? NSNumber == NSNumber(value: 0o640))
        #expect(try FileManager.default.contentsOfDirectory(atPath: fixture.target.deletingLastPathComponent().path) == ["SKILL.md"])
    }

    @Test("A stale revision reports typed conflict and keeps disk bytes and the draft")
    func conflict() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let checkout = try fixture.checkout()
        let loaded = try checkout.load(key: fixture.key)
        var draft = SkillDraft(document: loaded)
        try draft.setHolds(id: "01-question", holds: "draft")
        let candidate = try #require(try draft.render())
        let changed = Data("outside change\n".utf8)
        try changed.write(to: fixture.target)
        do {
            _ = try checkout.save(key: fixture.key, candidate: candidate, expectedRevision: loaded.revision)
            Issue.record("Saved a stale draft")
        } catch SkillSaveError.conflict(let path) { #expect(path == fixture.target.resolvingSymlinksInPath().path) }
        #expect(try Data(contentsOf: fixture.target) == changed)
        #expect(draft.isChanged)
    }

    @Test("The staged save checks the revision and path again and cleans its own stage")
    func stagedRecheck() throws {
        for changeLink in [false, true] {
            let fixture = try SkillCheckoutFixture()
            defer { fixture.clean() }
            let changed = Data("late external change\n".utf8)
            let outside = fixture.root.appendingPathComponent("outside.md")
            try changed.write(to: outside)
            let checkout = try SkillCheckout(path: fixture.root.appendingPathComponent("checkout").path,
                inventory: fixture.inventory, protectedRoots: [], beforeRecheck: {
                    if changeLink {
                        try? FileManager.default.removeItem(at: fixture.target)
                        try? FileManager.default.createSymbolicLink(at: fixture.target, withDestinationURL: outside)
                    } else {
                        try? changed.write(to: fixture.target)
                    }
                })
            let loaded = try checkout.load(key: fixture.key)
            do {
                _ = try checkout.save(key: fixture.key, candidate: Data("draft".utf8), expectedRevision: loaded.revision)
                Issue.record("A staged save missed the changed revision or path")
            } catch SkillSaveError.conflict { #expect(!changeLink) }
            catch SkillSaveError.path { #expect(changeLink) }
            #expect(try Data(contentsOf: fixture.target) == changed)
            #expect(try Data(contentsOf: outside) == changed)
            #expect(try FileManager.default.contentsOfDirectory(atPath: fixture.target.deletingLastPathComponent().path) == ["SKILL.md"])
        }
    }

    @Test("Missing, unreadable and nonregular targets refuse load and save")
    func missingAndUnreadable() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let checkout = try fixture.checkout()
        let loaded = try checkout.load(key: fixture.key)
        try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: fixture.target.path)
        #expect(throws: SkillSaveError.self) { try checkout.load(key: fixture.key) }
        #expect(throws: SkillSaveError.self) { try checkout.save(key: fixture.key, candidate: Data("candidate".utf8), expectedRevision: loaded.revision) }
        try FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: fixture.target.path)
        #expect(try Data(contentsOf: fixture.target) == loaded.bytes)
        try FileManager.default.removeItem(at: fixture.target)
        #expect(throws: SkillSaveError.self) { try checkout.save(key: fixture.key, candidate: loaded.bytes, expectedRevision: loaded.revision) }
        #expect(!FileManager.default.fileExists(atPath: fixture.target.path))
        try FileManager.default.createDirectory(at: fixture.target, withIntermediateDirectories: false)
        #expect(throws: SkillSaveError.self) { try checkout.load(key: fixture.key) }
    }

    @Test("Inventory, subtree, symlink and protected-root boundaries are enforced again")
    func trustBoundaries() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let checkout = try fixture.checkout()
        for key in ["kit/skills/../research/SKILL.md", "kit/skills/unknown/SKILL.md", "kit/vendor/research/SKILL.md", "/absolute/SKILL.md"] {
            #expect(throws: SkillSaveError.self) { try checkout.load(key: key) }
        }
        #expect(throws: SkillSaveError.self) { try fixture.checkout(protectedRoots: [fixture.root.appendingPathComponent("checkout")]) }
        let outside = fixture.root.appendingPathComponent("outside.md")
        let bytes = try Data(contentsOf: fixture.target)
        try bytes.write(to: outside)
        try FileManager.default.removeItem(at: fixture.target)
        try FileManager.default.createSymbolicLink(at: fixture.target, withDestinationURL: outside)
        #expect(throws: SkillSaveError.self) { try checkout.load(key: fixture.key) }
        #expect(throws: SkillSaveError.self) { try checkout.save(key: fixture.key, candidate: bytes, expectedRevision: SkillDocument.revision(of: bytes)) }
        #expect(try Data(contentsOf: outside) == bytes)
        let skills = fixture.root.appendingPathComponent("checkout/skills/kit/skills")
        try FileManager.default.removeItem(at: skills)
        try FileManager.default.createSymbolicLink(at: skills, withDestinationURL: fixture.bundle)
        #expect(throws: SkillSaveError.self) { try checkout.load(key: fixture.key) }
    }

    @Test("The real bundle and refreshed-home roots are protected by default")
    func defaultProtectedRoots() {
        for root in SkillCheckout.defaultProtectedRoots {
            do {
                _ = try SkillCheckout.validate(path: root.path)
                Issue.record("Accepted a protected default root")
            } catch SkillSaveError.path(_, let reason) { #expect(reason.contains("read-only")) }
            catch { Issue.record("Expected a typed protected path error") }
        }
    }

    @Test("Bundled and missing-checkout previews have explicit read-only document states")
    func sourceState() throws {
        let data = try SkillDocumentTests.fixture("research-like")
        let state = SkillDocumentState.bundledReadOnly(reason: "Set checkout.")
        let document = SkillDocument.parse(data: data, key: "research", state: state)
        #expect(document.state == state)
        var draft = SkillDraft(document: document)
        #expect(throws: SkillDraftIssue.self) { try draft.setHolds(id: "01-question", holds: "forbidden") }
        #expect(!draft.isChanged)
        let missing = SkillDocumentState.missingCheckoutSource(reason: "Missing source.")
        #expect(SkillDocument.parse(data: data, key: "research", state: missing).state == missing)
    }

    @Test("A stage write failure preserves the source and creates no staged leftovers")
    func stageFailure() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let checkout = try fixture.checkout()
        let loaded = try checkout.load(key: fixture.key)
        let directory = fixture.target.deletingLastPathComponent()
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: directory.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path) }
        #expect(throws: SkillSaveError.self) { try checkout.save(key: fixture.key, candidate: Data("new".utf8), expectedRevision: loaded.revision) }
        #expect(try Data(contentsOf: fixture.target) == loaded.bytes)
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path) == ["SKILL.md"])
    }

    @Test("Concurrent app saves with one revision have exactly one winner")
    func serializedSaves() async throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let checkout = try fixture.checkout()
        let loaded = try checkout.load(key: fixture.key)
        let results = await withTaskGroup(of: Bool.self, returning: [Bool].self) { group in
            for number in 1...2 {
                group.addTask {
                    do {
                        _ = try checkout.save(key: fixture.key, candidate: Data("writer \(number)".utf8), expectedRevision: loaded.revision)
                        return true
                    } catch { return false }
                }
            }
            var values: [Bool] = []
            for await result in group { values.append(result) }
            return values
        }
        #expect(results.filter { $0 }.count == 1)
    }
}

@Suite("Skills checkout setting")
@MainActor
struct SkillsCheckoutSettingTests {
    @Test("The optional preference defaults to nil and round-trips")
    func prefs() throws {
        #expect(try JSONDecoder().decode(Prefs.self, from: Data("{}".utf8)).skillsCheckout == nil)
        #expect(try JSONDecoder().decode(Prefs.self, from: Data(#"{"skillsCheckout":null}"#.utf8)).skillsCheckout == nil)
        let prefs = Prefs(skillsCheckout: "/source")
        #expect(try JSONDecoder().decode(Prefs.self, from: JSONEncoder().encode(prefs)) == prefs)
    }

    @Test("Settings saves the canonical path through choices and preserves other fields")
    func persist() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let choices = OwnerChoicesStore(folder: try claimedChoicesFolder(fixture.root.appendingPathComponent("home")))
        try choices.update { $0.pinned = ["/project"]; $0.prefs.theme = .dark; $0.prefs.notices.sound = false }
        let selection = SettingsSelection(choices: choices)
        selection.select(.skills)
        #expect(selection.setSkillsCheckout(fixture.root.appendingPathComponent("checkout").path))
        let saved = try choices.load()
        #expect(saved.prefs.skillsCheckout == fixture.root.appendingPathComponent("checkout").resolvingSymlinksInPath().path)
        #expect(saved.pinned == ["/project"] && saved.prefs.theme == .dark && !saved.prefs.notices.sound)
        #expect(SettingsSelection(choices: choices).prefs.skillsCheckout == saved.prefs.skillsCheckout)
        #expect(selection.setSkillsCheckout(nil))
        #expect(try choices.load().prefs.skillsCheckout == nil)
    }

    @Test("Bad paths and failed choices writes keep the old setting and use the Skills error")
    func failures() throws {
        let fixture = try SkillCheckoutFixture()
        defer { fixture.clean() }
        let home = try claimedChoicesFolder(fixture.root.appendingPathComponent("home"))
        let choices = OwnerChoicesStore(folder: home)
        let selection = SettingsSelection(choices: choices)
        selection.select(.skills)
        #expect(selection.setSkillsCheckout(fixture.root.appendingPathComponent("checkout").path))
        let previous = selection.prefs
        let bytes = try Data(contentsOf: home.appendingPathComponent("choices.json"))
        selection.setAppError("Unrelated error.")
        let revision = selection.pageErrorRevision
        #expect(!selection.setSkillsCheckout("relative"))
        #expect(selection.prefs == previous && selection.pageError != nil)
        #expect(selection.pageErrorRevision == revision + 1)
        #expect(selection.appError == "Unrelated error.")
        let lock = home.appendingPathComponent("choices.lock")
        try FileManager.default.removeItem(at: lock)
        try FileManager.default.createDirectory(at: lock, withIntermediateDirectories: false)
        let saveRevision = selection.saveErrorRevision
        let pageRevision = selection.pageErrorRevision
        #expect(!selection.setSkillsCheckout(nil))
        #expect(selection.saveError == nil)
        #expect(selection.saveErrorRevision == saveRevision)
        #expect(selection.pageErrorRevision == pageRevision + 1)
        #expect(selection.prefs == previous)
        #expect(selection.pageError?.contains("Could not save your settings") == true)
        #expect(try Data(contentsOf: home.appendingPathComponent("choices.json")) == bytes)
        try FileManager.default.removeItem(at: lock)
        #expect(selection.setSkillsCheckout(nil))
        #expect(selection.pageError == nil)
        #expect(selection.appError == "Unrelated error.")
    }
}
