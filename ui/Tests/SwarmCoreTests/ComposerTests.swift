import Foundation
import Testing
@testable import SwarmCore

private let CLAUDE = "claude"
private let CODEX = "codex"

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
        reopened.prune(keeping: ["chat-two"])
        #expect(reopened.draft(for: "chat-one").isEmpty)
        #expect(reopened.draft(for: "chat-two") == "second")
    }

    @Test("Mentions open at word starts")
    func menuTokens() {
        #expect(ComposerMenu.resolve(draft: "open @Sou", caret: 9, provider: CLAUDE) == .mention(
            ComposerToken(start: 5, length: 4, query: "Sou", reachesDraftEnd: true)
        ))
        #expect(ComposerMenu.resolve(draft: "mail@host", caret: 9, provider: CLAUDE) == .none)
    }

    @Test("A slash opens at any word start, with built-ins only at the draft start")
    func slashTokens() {
        #expect(ComposerMenu.resolve(draft: "please run /rev", caret: 15, provider: CLAUDE) == .slash(
            ComposerToken(start: 11, length: 4, query: "rev", reachesDraftEnd: true), .skillsAndCommands
        ))
        let wholeDraft = ComposerMenu.resolve(draft: "/cle", caret: 4, provider: CLAUDE)
        #expect(wholeDraft == .slash(
            ComposerToken(start: 0, length: 4, query: "cle", reachesDraftEnd: true), .all
        ))
        #expect(wholeDraft.isWholeDraft)
        #expect(ComposerMenu.resolve(draft: "src/a/b", caret: 7, provider: CLAUDE) == .none)
        #expect(ComposerMenu.resolve(draft: "(/rev", caret: 5, provider: CLAUDE) == .slash(
            ComposerToken(start: 1, length: 4, query: "rev", reachesDraftEnd: true), .skillsAndCommands
        ))
        let middle = ComposerMenu.resolve(draft: "fix /rev now", caret: 8, provider: CLAUDE)
        #expect(middle == .slash(ComposerToken(start: 4, length: 4, query: "rev"), .skillsAndCommands))
        #expect(!middle.isWholeDraft)
        #expect(!ComposerMenu.resolve(draft: "/cle now", caret: 4, provider: CLAUDE).isWholeDraft)
    }

    @Test("Codex opens a slash only at the draft start and a dollar skill anywhere")
    func codexTokens() {
        #expect(ComposerMenu.resolve(draft: "please run /rev", caret: 15, provider: CODEX) == .none)
        #expect(ComposerMenu.resolve(draft: "/rev", caret: 4, provider: CODEX) == .slash(
            ComposerToken(start: 0, length: 4, query: "rev", reachesDraftEnd: true), .all
        ))
        #expect(ComposerMenu.resolve(draft: "use $tdd", caret: 8, provider: CODEX) == .skill(
            ComposerToken(start: 4, length: 4, query: "tdd", reachesDraftEnd: true)
        ))
        #expect(ComposerMenu.resolve(draft: "use $tdd", caret: 8, provider: CLAUDE) == .none)
        #expect(ComposerMenu.resolve(draft: "cost5$tdd", caret: 9, provider: CODEX) == .none)
    }

    @Test("A mid-draft menu offers no built-ins and a dollar menu offers only skills")
    func menuScopes() {
        let commands = [
            ComposerCommand(name: "clear", detail: "", kind: .builtIn),
            ComposerCommand(name: "ship", detail: "", kind: .command),
            ComposerCommand(name: "tdd", detail: "", kind: .skill, path: "/skills/tdd/SKILL.md"),
            ComposerCommand(
                name: "ponytail:ponytail", detail: "", kind: .plugin("ponytail"),
                path: "/plugins/ponytail/skills/ponytail/SKILL.md"
            ),
            ComposerCommand(
                name: "ponytail:audit", detail: "", kind: .plugin("ponytail"),
                path: "/plugins/ponytail/commands/audit.md"
            ),
        ]
        let token = ComposerToken(start: 4, length: 1, query: "")
        #expect(ComposerMenu.slash(token, .all).offered(commands) == commands)
        #expect(ComposerMenu.slash(token, .skillsAndCommands).offered(commands).map(\.name)
            == ["ship", "tdd", "ponytail:ponytail", "ponytail:audit"])
        #expect(ComposerMenu.skill(token).offered(commands).map(\.name) == ["tdd", "ponytail:ponytail"])
        #expect(ComposerMenu.skill(token).insertion(for: commands[2]) == "$tdd")
        #expect(ComposerMenu.slash(token, .skillsAndCommands).insertion(for: commands[2]) == "/tdd")
        #expect(ComposerMenu.slash(token, .all).showsWhenEmpty)
        #expect(!ComposerMenu.slash(token, .skillsAndCommands).showsWhenEmpty)
        #expect(!ComposerMenu.skill(token).showsWhenEmpty)
    }

    @Test("A pick replaces only its token")
    func insertion() {
        let token = ComposerToken(start: 5, length: 4, query: "Sou")
        #expect(ComposerMenu.inserting(
            "@Sources/App.swift", into: "open @Sou now", token: token
        ) == "open @Sources/App.swift now")
    }

    @Test("A pick at a cursor in the middle keeps the text after the token")
    func insertionAtCursor() throws {
        let draft = "fix /rev now"
        let token = try #require(ComposerMenu.resolve(draft: draft, caret: 8, provider: CLAUDE).token)
        #expect(ComposerMenu.inserting("/review", into: draft, token: token) == "fix /review now")
    }

    @Test("Menu keys move, pick, dismiss, and preserve composer keys")
    func keyRouting() {
        #expect(route(.down, menuOpen: true, hasRows: true) == .move(1))
        #expect(route(.return, menuOpen: true, hasRows: true) == .pick)
        #expect(route(.tab, menuOpen: true, hasRows: true) == .pick)
        #expect(route(.escape, menuOpen: true, hasRows: true) == .dismissMenu)
        #expect(route(.return, menuOpen: false, hasRows: false) == .send)
        #expect(route(.shiftReturn, menuOpen: false, hasRows: false) == .insertNewline)
        #expect(route(.escape, menuOpen: false, hasRows: false) == .clear)
        #expect(ComposerKeyRouter.movedSelection(current: 0, count: 3, delta: -1) == 2)
    }

    @Test("Return on a whole-draft command picks and sends, and mid-draft it only picks")
    func returnPicks() {
        let wholeDraft = ComposerMenu.resolve(draft: "/cle", caret: 4, provider: CLAUDE)
        let midDraft = ComposerMenu.resolve(draft: "please run /rev", caret: 15, provider: CLAUDE)
        #expect(route(.return, menu: wholeDraft, menuOpen: true, hasRows: true) == .pickAndSend)
        #expect(route(.tab, menu: wholeDraft, menuOpen: true, hasRows: true) == .pick)
        #expect(route(.return, menu: midDraft, menuOpen: true, hasRows: true) == .pick)
    }

    @Test("Up in an empty draft pulls back only when the chat can")
    func upPullsBack() {
        #expect(route(.up, menuOpen: false, hasRows: false, draftIsEmpty: true, canPullBack: true)
            == .pullBack)
        #expect(route(.up, menuOpen: false, hasRows: false, draftIsEmpty: false, canPullBack: true)
            == .move(0))
        #expect(route(.up, menuOpen: false, hasRows: false, draftIsEmpty: true, canPullBack: false)
            == .move(0))
        #expect(route(.up, menuOpen: true, hasRows: true, draftIsEmpty: true, canPullBack: true)
            == .move(-1))
    }

    @Test("A plain sentence is typed unchanged")
    func submissionKeepsPlainText() {
        let sentence = "Add a test for the empty case, then mail me@host.\nThanks"
        #expect(Composer.submission(sentence) == sentence)
    }

    @Test("Invisible format characters are removed before typing")
    func submissionRemovesInvisibleCharacters() {
        #expect(Composer.submission("a\u{00AD}b\u{200B}c\u{200C}d\u{200D}e\u{200E}f\u{200F}g") == "abcdefg")
        #expect(Composer.submission("h\u{2060}i\u{2061}j\u{2062}k\u{2063}l\u{2064}m\u{FEFF}n") == "hijklmn")
    }

    @Test("CR and CRLF become LF, because a CR is Enter")
    func submissionNormalizesLineEnds() {
        #expect(Composer.submission("one\r\ntwo\rthree\nfour") == "one\ntwo\nthree\nfour")
    }

    @Test("A trailing backslash or a last word that opens a popup gets one space")
    func submissionClosesPopups() {
        #expect(Composer.submission("join this\\") == "join this\\ ")
        #expect(Composer.submission("look at @src/main.rs") == "look at @src/main.rs ")
        #expect(Composer.submission("use $tdd") == "use $tdd ")
        #expect(Composer.submission("/clear") == "/clear ")
        #expect(Composer.submission("please run /review") == "please run /review ")
        #expect(Composer.submission("ship it :tada:") == "ship it :tada: ")
        #expect(Composer.submission("Note:") == "Note: ")
        #expect(Composer.submission("first line\nsee @README.md") == "first line\nsee @README.md ")
        #expect(Composer.submission("look at @src/main.rs\u{200B}") == "look at @src/main.rs ")
    }

    @Test("A waiting agent blocks typing, and the chair's reason points to its pane")
    func waitingAgentBlocksTyping() {
        #expect(SwarmSessionInteraction.questionReason(status: .waiting, target: .agent)
            == "The agent is asking a question. Answer it first.")
        #expect(SwarmSessionInteraction.questionReason(status: .waiting, target: .chair)
            == "The agent is asking a question. Answer it first in its pane.")
        for status in [AgentStatus.working, .done, .failed, .ended] {
            #expect(SwarmSessionInteraction.questionReason(status: status, target: .chair) == nil)
        }
        #expect(SwarmSessionInteraction.questionReason(status: nil, target: .chair) == nil)
    }

    @Test("An empty completion menu lets Return send")
    func emptyMenuKeys() {
        #expect(route(.return, menuOpen: true, hasRows: false) == .send)
        #expect(route(.tab, menuOpen: true, hasRows: false) == .move(0))
        #expect(route(.up, menuOpen: true, hasRows: false) == .move(0))
        #expect(route(.down, menuOpen: true, hasRows: false) == .move(0))
        #expect(route(.escape, menuOpen: true, hasRows: false) == .dismissMenu)
    }

    private func route(
        _ key: ComposerInputKey, menu: ComposerMenu = .none, menuOpen: Bool, hasRows: Bool,
        draftIsEmpty: Bool = false, canPullBack: Bool = false
    ) -> ComposerKeyAction {
        ComposerKeyRouter.route(
            key, menu: menu, menuOpen: menuOpen, hasRows: hasRows,
            draftIsEmpty: draftIsEmpty, canPullBack: canPullBack
        )
    }

    @Test("Command matching is fuzzy and provider built-ins differ")
    func commandMatching() {
        let codex = ComposerCommandCatalog.builtIns(provider: "codex")
        let claude = ComposerCommandCatalog.builtIns(provider: "claude")
        #expect(codex.contains { $0.name == "new" })
        #expect(!claude.contains { $0.name == "new" })
        #expect(ComposerCommandCatalog.matches(codex, query: "rv").first?.command.name == "review")
    }

    @Test("A Codex chair sees Codex roots, and a Claude chair sees Claude and project roots")
    func commandDiscovery() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ComposerCatalog-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let command = root.appendingPathComponent(".claude/commands/team/check.md")
        let skill = root.appendingPathComponent(".claude/skills/explain/SKILL.md")
        let prompt = root.appendingPathComponent(".codex/prompts/ship.md")
        let project = root.appendingPathComponent("project/.claude/commands/team/check.md")
        let projectSkill = root.appendingPathComponent("project/.claude/skills/explain/SKILL.md")
        let agentSkill = root.appendingPathComponent(".agents/skills/shared/SKILL.md")
        let accountPrompt = root.appendingPathComponent("account/prompts/account.md")
        for url in [command, skill, prompt, project, projectSkill, agentSkill, accountPrompt] {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true
            )
        }
        try "---\ndescription: Check changes\n---\n".write(to: command, atomically: true, encoding: .utf8)
        try "---\nname: explain\ndescription: Explain the code\n---\n".write(to: skill, atomically: true, encoding: .utf8)
        try "---\ndescription: Ship safely\n---\n".write(to: prompt, atomically: true, encoding: .utf8)
        try "---\ndescription: Project check\n---\n".write(to: project, atomically: true, encoding: .utf8)
        try "---\ndescription: Project explain\n---\n".write(to: projectSkill, atomically: true, encoding: .utf8)
        try "---\ndescription: Shared skill\n---\n".write(to: agentSkill, atomically: true, encoding: .utf8)
        try "---\ndescription: Account prompt\n---\n".write(to: accountPrompt, atomically: true, encoding: .utf8)

        let codex = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "codex", homeDirectory: root.path
        ))
        #expect(!codex.contains { $0.name == "team:check" || $0.name == "explain" })
        #expect(codex.contains { $0.name == "ship" && $0.detail == "Ship safely" })
        #expect(codex.contains { $0.name == "shared" })
        let accountCodex = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "codex", homeDirectory: root.path,
            configDirectory: root.appendingPathComponent("account").path
        ))
        #expect(accountCodex.contains { $0.name == "account" })
        #expect(!accountCodex.contains { $0.name == "ship" })
        let claude = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "claude", homeDirectory: root.path,
            projectDirectory: root.appendingPathComponent("project").path
        ))
        #expect(!claude.contains { $0.name == "ship" })
        #expect(claude.contains { $0.name == "team:check" && $0.detail == "Project check" })
        #expect(claude.contains {
            $0.name == "explain" && $0.kind == .skill && $0.detail == "Project explain"
        })
        #expect(claude.contains { $0.name == "shared" })
        let unknown = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: nil, homeDirectory: root.path
        ))
        #expect(unknown.contains { $0.name == "ship" })
        #expect(unknown.contains { $0.name == "team:check" })
    }

    @Test("A chair log selects its account config root")
    func accountRoot() {
        let session = SwarmSession(
            id: SwarmSessionID("one"), talkMode: "solo", adapter: nil,
            cwd: "/work", createdAt: 0, chairLog: "/users/a/logs/one.jsonl",
            agents: 0, messages: 0, lastMessageAt: nil
        )
        let account = SwarmAccount(
            name: "a", email: nil, home: "/users/a",
            env: ["CLAUDE_CONFIG_DIR": "/profiles/a/claude"],
            signedIn: true, remainingPct: nil, summary: nil
        )
        let source = ComposerCommandSource.resolve(
            provider: "claude", session: session, accounts: [account], homeDirectory: "/users/default"
        )
        #expect(source.configDirectory == "/profiles/a/claude")
        var other = session
        other.chairLog = "/users/another/log.jsonl"
        #expect(ComposerCommandSource.resolve(
            provider: "claude", session: other, accounts: [account],
            homeDirectory: "/users/default"
        ).configDirectory == nil)
        let codex = SwarmAccount(
            name: "codex", email: nil, home: "/users/a",
            env: ["CODEX_HOME": "/profiles/a/codex"],
            signedIn: true, remainingPct: nil, summary: nil
        )
        #expect(ComposerCommandSource.resolve(
            provider: "codex", session: session, accounts: [codex],
            homeDirectory: "/users/default"
        ).configDirectory == "/profiles/a/codex")
        var noEnvironment = codex
        noEnvironment.env = [:]
        #expect(ComposerCommandSource.resolve(
            provider: "codex", session: session, accounts: [noEnvironment],
            homeDirectory: "/users/default"
        ).configDirectory == "/users/a")
    }

    @Test("Block descriptions and a cut UTF-8 character keep frontmatter")
    func blockDescriptions() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ComposerYAML-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let commands = root.appendingPathComponent(".claude/commands")
        try FileManager.default.createDirectory(at: commands, withIntermediateDirectories: true)
        let literal = commands.appendingPathComponent("literal.md")
        let folded = commands.appendingPathComponent("folded.md")
        let cut = commands.appendingPathComponent("cut.md")
        try "---\ndescription: |\n  First line\n  Second line\n---\n".write(
            to: literal, atomically: true, encoding: .utf8
        )
        try "---\ndescription: >\n  First line\n  Second line\n---\n".write(
            to: folded, atomically: true, encoding: .utf8
        )
        try "---\ndescription: >-\n  Kept line\n---\n".write(
            to: commands.appendingPathComponent("chomped.md"), atomically: true, encoding: .utf8
        )
        let header = "---\ndescription: Survives cut\n---\n"
        let text = header + String(repeating: "x", count: 8_191 - header.utf8.count) + "é"
        try text.write(to: cut, atomically: true, encoding: .utf8)
        let found = ComposerCommandCatalog.discover(from: ComposerCommandSource(
            provider: "claude", homeDirectory: root.path
        ))
        #expect(found.first { $0.name == "literal" }?.detail == "First line\nSecond line")
        #expect(found.first { $0.name == "folded" }?.detail == "First line Second line")
        #expect(found.first { $0.name == "chomped" }?.detail == "Kept line")
        #expect(found.first { $0.name == "cut" }?.detail == "Survives cut")
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
        let spaced = Composer.appending(path: "/tmp/a file.txt", to: "look")
        #expect(spaced == "look \"/tmp/a file.txt\" ")
        #expect(Composer.contains(path: "/tmp/a file.txt", in: spaced))
        #expect(Composer.removing(path: "/tmp/a file.txt", from: spaced) == "look ")
        #expect(Composer.retainedAttachments(
            [ComposerAttachment(path: "/tmp/a file.txt")], in: spaced
        ).count == 1)
    }

    @Test("A late attachment needs the same chat and generation")
    func attachmentContext() {
        let first = ComposerAttachmentContext(sessionID: "one", generation: 2)
        let second = ComposerAttachmentContext(sessionID: "one", generation: 2)
        #expect(first.matches(sessionID: "one", generation: 2))
        #expect(second.matches(sessionID: "one", generation: 2))
        #expect(!first.matches(sessionID: "two", generation: 2))
        #expect(!first.matches(sessionID: "one", generation: 3))
    }

    @Test("A chip needs a whole path, including paths with spaces")
    func wholeAttachmentPaths() {
        let path = "/tmp/a.txt"
        let spacedPath = "/tmp/a file.txt"
        #expect(!Composer.contains(path: path, in: "/tmp/a.txt.bak"))
        #expect(Composer.removing(path: path, from: "/tmp/a.txt.bak") == "/tmp/a.txt.bak")
        #expect(Composer.contains(path: spacedPath, in: "open /tmp/a file.txt now"))
        #expect(Composer.removing(path: spacedPath, from: "open /tmp/a file.txt now") == "open now")
        #expect(Composer.contains(path: "a a", in: "xa a a"))
        #expect(Composer.removing(path: "a a", from: "xa a a") == "xa ")
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

    @Test("A failed send keeps the submitted draft and another chat's draft")
    func failedSendKeepsDrafts() {
        var state = ComposerSendState()
        #expect(state.begin(sessionID: "one", draft: "first") == "first")
        #expect(state.begin(sessionID: "two", draft: "second") == "second")
        #expect(state.finish(sessionID: "one", currentDraft: "first", succeeded: false) == "first")
        #expect(!state.isSending(sessionID: "one"))
        #expect(state.isSending(sessionID: "two"))
        #expect(state.finish(sessionID: "two", currentDraft: "edited second", succeeded: true)
            == "edited second")
    }

    @Test("A missing file or folder cannot become an attachment token")
    func invalidAttachmentSources() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ComposerAttachments-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        #expect(throws: (any Error).self) {
            try ComposerAttachmentStore.importFile(
                at: root.appendingPathComponent("missing.txt").path, scratchDirectory: root.path
            )
        }
        #expect(throws: ComposerAttachmentError.self) {
            try ComposerAttachmentStore.importFile(at: root.path, scratchDirectory: root.path)
        }
        let file = root.appendingPathComponent("readable.txt")
        try "data".write(to: file, atomically: true, encoding: .utf8)
        #expect(try ComposerAttachmentStore.importFile(
            at: file.path, scratchDirectory: root.path
        ).path == file.path)
    }
}
