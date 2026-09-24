import Foundation
import Testing
@testable import SwarmCore

@Suite("Composer")
struct ComposerTests {
    @Test("Drafts are separate for each chat and survive a new store")
    func persistedDrafts() throws {
        let suite = "ComposerTests-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }

        ComposerDraftStore(defaults: defaults).save("first", for: "chat-one")
        ComposerDraftStore(defaults: defaults).save("second", for: "chat-two")

        let reopened = ComposerDraftStore(defaults: defaults)
        #expect(reopened.draft(for: "chat-one") == "first")
        #expect(reopened.draft(for: "chat-two") == "second")
    }

    @Test("A slash menu opens only at the start and mentions open at word starts")
    func menuTokens() {
        #expect(ComposerMenu.resolve(draft: "/rev", caret: 4) == .slash(
            ComposerToken(start: 0, length: 4, query: "rev")
        ))
        #expect(ComposerMenu.resolve(draft: "do /rev", caret: 7) == .none)
        #expect(ComposerMenu.resolve(draft: "open @Sou", caret: 9) == .mention(
            ComposerToken(start: 5, length: 4, query: "Sou")
        ))
        #expect(ComposerMenu.resolve(draft: "mail@host", caret: 9) == .none)
    }

    @Test("A pick replaces only its token")
    func insertion() {
        let token = ComposerToken(start: 5, length: 4, query: "Sou")
        #expect(ComposerMenu.inserting(
            "@Sources/App.swift", into: "open @Sou now", token: token
        ) == "open @Sources/App.swift now")
    }

    @Test("Menu keys move, pick, dismiss, and preserve composer keys")
    func keyRouting() {
        #expect(ComposerKeyRouter.route(.down, menuOpen: true, hasRows: true) == .move(1))
        #expect(ComposerKeyRouter.route(.return, menuOpen: true, hasRows: true) == .pick)
        #expect(ComposerKeyRouter.route(.tab, menuOpen: true, hasRows: true) == .pick)
        #expect(ComposerKeyRouter.route(.escape, menuOpen: true, hasRows: true) == .dismissMenu)
        #expect(ComposerKeyRouter.route(.return, menuOpen: false, hasRows: false) == .send)
        #expect(ComposerKeyRouter.route(.shiftReturn, menuOpen: false, hasRows: false) == .insertNewline)
        #expect(ComposerKeyRouter.route(.escape, menuOpen: false, hasRows: false) == .clear)
        #expect(ComposerKeyRouter.movedSelection(current: 0, count: 3, delta: -1) == 2)
    }

    @Test("An empty completion menu lets Return send")
    func emptyMenuKeys() {
        #expect(ComposerKeyRouter.route(.return, menuOpen: true, hasRows: false) == .send)
        #expect(ComposerKeyRouter.route(.tab, menuOpen: true, hasRows: false) == .move(0))
        #expect(ComposerKeyRouter.route(.up, menuOpen: true, hasRows: false) == .move(0))
        #expect(ComposerKeyRouter.route(.down, menuOpen: true, hasRows: false) == .move(0))
        #expect(ComposerKeyRouter.route(.escape, menuOpen: true, hasRows: false) == .dismissMenu)
    }

    @Test("Command matching is fuzzy and provider built-ins differ")
    func commandMatching() {
        let codex = ComposerCommandCatalog.builtIns(provider: "codex")
        let claude = ComposerCommandCatalog.builtIns(provider: "claude")
        #expect(codex.contains { $0.name == "new" })
        #expect(!claude.contains { $0.name == "new" })
        #expect(ComposerCommandCatalog.matches(codex, query: "rv").first?.command.name == "review")
    }

    @Test("User commands, skills, and Codex prompts are discovered with descriptions")
    func commandDiscovery() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ComposerCatalog-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let command = root.appendingPathComponent(".claude/commands/team/check.md")
        let skill = root.appendingPathComponent(".claude/skills/explain/SKILL.md")
        let prompt = root.appendingPathComponent(".codex/prompts/ship.md")
        for url in [command, skill, prompt] {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true
            )
        }
        try "---\ndescription: Check changes\n---\n".write(to: command, atomically: true, encoding: .utf8)
        try "---\nname: explain\ndescription: Explain the code\n---\n".write(to: skill, atomically: true, encoding: .utf8)
        try "---\ndescription: Ship safely\n---\n".write(to: prompt, atomically: true, encoding: .utf8)

        let commands = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "codex", homeDirectory: root.path
        ))
        #expect(commands.contains { $0.name == "team:check" && $0.detail == "Check changes" })
        #expect(commands.contains { $0.name == "explain" && $0.kind == .skill })
        #expect(commands.contains { $0.name == "ship" && $0.detail == "Ship safely" })
    }

    @Test("File matching favors a file-name hit")
    func fileMatching() {
        let paths = ["Sources/Review/Panel.swift", "Sources/AppReview.swift", "README.md"]
        let matches = ComposerFileCatalog.matches(paths, query: "review")
        #expect(matches.first?.path == "Sources/AppReview.swift")
    }

    @Test("Paths append and remove without changing surrounding text")
    func pathDrafts() {
        let draft = Composer.appending(path: "/tmp/image.png", to: "look")
        #expect(draft == "look /tmp/image.png ")
        #expect(Composer.removing(path: "/tmp/image.png", from: draft) == "look ")
    }

    @Test("A chip needs a whole path, including paths with spaces")
    func wholeAttachmentPaths() {
        let path = "/tmp/a.txt"
        let spacedPath = "/tmp/a file.txt"
        #expect(!Composer.contains(path: path, in: "/tmp/a.txt.bak"))
        #expect(Composer.removing(path: path, from: "/tmp/a.txt.bak") == "/tmp/a.txt.bak")
        #expect(Composer.contains(path: spacedPath, in: "open /tmp/a file.txt now"))
        #expect(Composer.removing(path: spacedPath, from: "open /tmp/a file.txt now") == "open now")
        #expect(Composer.retainedAttachments(
            [ComposerAttachment(path: path), ComposerAttachment(path: spacedPath)],
            in: "open /tmp/a file.txt now"
        ) == [ComposerAttachment(path: spacedPath)])
    }

    @Test("A file added while a send runs keeps its chip")
    func attachmentAddedDuringSend() {
        var state = ComposerSendState()
        let attachment = ComposerAttachment(path: "/tmp/new file.txt")
        #expect(state.begin(sessionID: "one", draft: "hello") == "hello")
        let edited = Composer.appending(path: attachment.path, to: "hello")
        let finished = state.finish(sessionID: "one", currentDraft: edited, succeeded: true)
        #expect(finished == edited)
        #expect(Composer.retainedAttachments([attachment], in: finished) == [attachment])
    }
}
