import Foundation
import Testing
@testable import SwarmCore

@Suite("Skill draft")
struct SkillDraftTests {
    func draft(_ role: String = "multibyte") throws -> SkillDraft {
        SkillDraft(document: SkillDocument.parse(data: try SkillDocumentTests.fixture(role), key: role))
    }

    @Test("No-op and reverted edits return no candidate")
    func noOp() throws {
        var value = try draft()
        #expect(!value.isChanged)
        #expect(try value.render() == nil)
        let holds = value.steps[0].holds
        try value.setHolds(id: "01-question", holds: "new")
        #expect(value.isChanged)
        try value.setHolds(id: "01-question", holds: holds)
        #expect(try value.render() == nil)
        try value.rename(id: "01-question", name: "ask")
        try value.rename(id: "01-question", name: "question")
        #expect(try value.render() == nil)
    }

    @Test("A body edit leaves the table and all other bytes equal")
    func bodyOnly() throws {
        for role in ["multibyte", "crlf", "two-tables", "fenced-heading"] {
            var value = try draft(role)
            let original = value.document
            let section = try #require(original.sections["01-question"])
            try value.setBody(id: "01-question", body: "Edited α\n### Details\n~~~\n## fenced heading\n~~~\n")
            let candidate = try #require(try value.render())
            #expect(candidate.prefix(section.bodyRange.lowerBound) == original.bytes.prefix(section.bodyRange.lowerBound))
            #expect(candidate.suffix(original.bytes.count - section.bodyRange.upperBound) == original.bytes.suffix(original.bytes.count - section.bodyRange.upperBound))
            let parsed = SkillDocument.parse(data: candidate, key: role)
            #expect(parsed.rows == original.rows)
            #expect(parsed.sections["01-question"]?.body.contains("Edited α") == true)
            if role == "crlf" {
                #expect(parsed.sections["01-question"]?.body == "Edited α\r\n### Details\r\n~~~\r\n## fenced heading\r\n~~~\r\n")
            }
        }
    }

    @Test("Rename keeps dependency qualifiers, stable ids and unrelated prose")
    func rename() throws {
        var value = try draft("two-tables")
        try value.rename(id: "01-question", name: "ask")
        #expect(value.steps[0].id == "01-question")
        #expect(value.steps[1].needs == ["01-question"])
        let candidate = try #require(try value.render())
        let text = String(decoding: candidate, as: UTF8.self)
        #expect(text.contains("ask unless skipped"))
        #expect(text.contains("## 01-ask"))
        #expect(text.contains("| `99-later.md` | none | untouched |"))
        #expect(text.contains("## Guards\nKEEP"))
        #expect(value.validate().isEmpty)
    }

    @Test("A Holds edit preserves every byte outside the first table and all unchanged rows")
    func tableOnly() throws {
        for role in ["multibyte", "crlf", "two-tables"] {
            var value = try draft(role)
            let original = value.document
            let range = try #require(original.tableRange)
            try value.setHolds(id: "02-report", holds: "changed")
            let candidate = try #require(try value.render())
            #expect(candidate.prefix(range.lowerBound) == original.bytes.prefix(range.lowerBound))
            #expect(candidate.suffix(original.bytes.count - range.upperBound) == original.bytes.suffix(original.bytes.count - range.upperBound))
            let parsed = SkillDocument.parse(data: candidate, key: role)
            #expect(candidate.subdata(in: parsed.rows[0].range) == original.bytes.subdata(in: original.rows[0].range))
        }
    }

    @Test("The row count is bounded and no-final-newline survives a no-op")
    func rowBoundaries() throws {
        let prefix = "python3 <kit>/references/step_run.py start folder skill\n| File | Needs | Holds |\n|---|---|---|\n"
        let rows = (1...99).map { "| \(String(format: "%02d", $0))-step\($0).md | none | |\n" }.joined()
        var full = SkillDraft(document: SkillDocument.parse(data: Data((prefix + rows).utf8), key: "full"))
        #expect(throws: SkillDraftIssue.self) { try full.add(name: "extra") }
        #expect(!full.isChanged)
        var single = SkillDraft(document: SkillDocument.parse(data: Data((prefix + "| 01-first.md | none | |").utf8), key: "single"))
        #expect(try single.render() == nil)
        #expect(throws: SkillDraftIssue.self) { try single.remove(id: "01-first") }
        #expect(!single.isChanged)
        try single.add(name: "second")
        #expect(SkillDocument.parse(data: try #require(try single.render()), key: "single").rows.count == 2)
        var headingAtEOF = SkillDraft(document: SkillDocument.parse(data: Data((prefix + "| 01-first.md | none | |\n\n## 01-first").utf8), key: "heading"))
        try headingAtEOF.setBody(id: "01-first", body: "new body")
        let candidate = try #require(try headingAtEOF.render())
        #expect(String(decoding: candidate, as: UTF8.self).hasSuffix("## 01-first\nnew body"))
    }

    @Test("Reorder renumbers rows and headings without moving prose")
    func reorder() throws {
        var value = try draft()
        try value.move(id: "02-report", to: 0)
        #expect(value.steps.map(\.id) == ["02-report", "01-question"])
        #expect(value.steps.map(\.stem) == ["01-report", "02-question"])
        let candidate = try #require(try value.render())
        let text = String(decoding: candidate, as: UTF8.self)
        #expect(text.range(of: "## 02-question")!.lowerBound < text.range(of: "## 01-report")!.lowerBound)
        #expect(text.contains("question unless skipped"))
        #expect(value.steps[0].needs == ["01-question"])
    }

    @Test("Add and remove enforce dependency and section confirmation rules")
    func addRemove() throws {
        var value = try draft()
        #expect(throws: SkillDraftIssue.self) { try value.remove(id: "01-question", removeSection: true) }
        #expect(throws: SkillDraftIssue.self) { try value.remove(id: "02-report") }
        try value.remove(id: "02-report", removeSection: true)
        let added = try value.add(name: "check", body: "Check text\n")
        #expect(value.steps.last?.id == added)
        try value.setNeeds(id: added, needs: ["01-question"])
        let text = String(decoding: try #require(try value.render()), as: UTF8.self)
        #expect(!text.contains("## 02-report"))
        #expect(text.contains("## 02-check\nCheck text"))
        #expect(text.contains("## Guards\nKEEP"))
        var noSection = try draft("council-like")
        #expect(throws: SkillDraftIssue.self) { try noSection.setBody(id: "01-brief", body: "new") }
        let rowOnly = try noSection.add(name: "extra")
        #expect(noSection.steps.first { $0.id == rowOnly }?.body == nil)
    }

    @Test("Script-owned structural operations are locked in core")
    func scriptLock() throws {
        var value = try draft("flow-like")
        #expect(throws: SkillDraftIssue.self) { try value.add(name: "extra") }
        #expect(throws: SkillDraftIssue.self) { try value.remove(id: "07-close") }
        #expect(throws: SkillDraftIssue.self) { try value.rename(id: "01-frame", name: "first") }
        #expect(throws: SkillDraftIssue.self) { try value.move(id: "07-close", to: 0) }
        try value.setHolds(id: "01-frame", holds: "changed")
        try value.setNeeds(id: "07-close", needs: ["01-frame"])
        #expect(try value.render() != nil)
    }

    @Test("Invalid names, cells, bodies and edges are rejected without a draft change")
    func validation() throws {
        var value = try draft()
        for name in ["", "none", "and", "../path", "Upper", "with space", "with_pipe|", "a--b", "report"] {
            #expect(throws: SkillDraftIssue.self) { try value.rename(id: "01-question", name: name) }
        }
        for holds in ["pipe|", "two\nlines", "tab\t", "control\u{7F}"] {
            #expect(throws: SkillDraftIssue.self) { try value.setHolds(id: "01-question", holds: holds) }
        }
        for body in ["# New\n", "## New\n", "##\n", "---\n## New\n", "~~~\ninside\n~~~\n## New\n", "```\nopen fence"] {
            #expect(throws: SkillDraftIssue.self) { try value.setBody(id: "01-question", body: body) }
        }
        #expect(throws: SkillDraftIssue.self) { try value.setNeeds(id: "01-question", needs: ["missing"]) }
        #expect(throws: SkillDraftIssue.self) { try value.setNeeds(id: "01-question", needs: ["01-question"]) }
        #expect(throws: SkillDraftIssue.self) { try value.setNeeds(id: "02-report", needs: ["01-question", "01-question"]) }
        #expect(throws: SkillDraftIssue.self) { try value.setNeeds(id: "01-question", needs: ["02-report"]) }
        #expect(!value.isChanged)
        let extra = try value.add(name: "report-extra")
        #expect(throws: SkillDraftIssue.self) { try value.setNeeds(id: "01-question", needs: ["02-report"]) }
        try value.setNeeds(id: "01-question", needs: [extra])
        #expect(value.validate().isEmpty)
    }

    @Test("Added and renamed steps use their current display name in field issues")
    func issueDisplayNames() throws {
        var value = try draft()
        let added = try value.add(name: "check", body: "Check text\n")
        let display = try #require(value.steps.first { $0.id == added }?.stem)
        #expect(throws: SkillDraftIssue.invalidHolds(id: display)) { try value.setHolds(id: added, holds: "a|b") }
        #expect(throws: SkillDraftIssue.invalidBody(id: display)) { try value.setBody(id: added, body: "## New\n") }
        #expect(throws: SkillDraftIssue.unknownNeed(id: display, need: "missing")) { try value.setNeeds(id: added, needs: ["missing"]) }
        #expect(throws: SkillDraftIssue.selfNeed(id: display)) { try value.setNeeds(id: added, needs: [added]) }
        #expect(throws: SkillDraftIssue.duplicateNeed(id: display)) { try value.setNeeds(id: added, needs: ["01-question", "01-question"]) }
        try value.rename(id: "01-question", name: "ask")
        #expect(throws: SkillDraftIssue.invalidHolds(id: "01-ask")) { try value.setHolds(id: "01-question", holds: "a|b") }
        #expect(throws: SkillDraftIssue.invalidBody(id: "01-ask")) { try value.setBody(id: "01-question", body: "## New\n") }
        #expect(throws: SkillDraftIssue.selfNeed(id: "01-ask")) { try value.setNeeds(id: "01-question", needs: ["01-question"]) }
        try value.add(name: "report-extra")
        #expect(throws: SkillDraftIssue.ambiguousNeeds(id: display, word: "report")) { try value.setNeeds(id: added, needs: ["02-report"]) }
    }

    @Test("Holds padding and one outer backtick pair round-trip without a render mismatch")
    func normalizedHolds() throws {
        for input in ["  Ready to save  ", "`Ready to save`", "  `Ready to save`  "] {
            var value = try draft()
            try value.setHolds(id: "01-question", holds: input)
            #expect(value.steps.first?.holds == "Ready to save")
            let candidate = try #require(try value.render())
            #expect(SkillDocument.parse(data: candidate, key: "normalized-holds").rows.first?.holds == "Ready to save")
        }
    }

    @Test("Holds that still lose text in parsing fail with a field issue")
    func unrepresentableHolds() throws {
        var value = try draft()
        for input in ["``Ready``", "`Ready", "Ready`", "` Ready `"] {
            do {
                try value.setHolds(id: "01-question", holds: input)
                Issue.record("Holds that changes during parsing must be rejected")
            } catch {
                #expect(error.localizedDescription.contains("Holds for 01-question"))
                #expect(error.localizedDescription.contains("spaces or backticks"))
            }
        }
        #expect(!value.isChanged)
    }

    @Test("Rendered output agrees with kit table and need_files")
    func parity() throws {
        var value = try draft()
        try value.rename(id: "01-question", name: "ask")
        let added = try value.add(name: "check", body: "Check\n")
        try value.setNeeds(id: added, needs: ["02-report", "01-question"])
        try value.move(id: added, to: 0)
        let candidate = try #require(try value.render())
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".md")
        defer { try? FileManager.default.removeItem(at: file) }
        try candidate.write(to: file)
        let repo = SkillDocumentTests.root.deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = ["-I", "-c", "import runpy,json,sys; m=runpy.run_path(sys.argv[1]); rows=m['table'](sys.argv[2]); print(json.dumps([[*r,m['need_files'](r[1],[s[0] for s in rows])] for r in rows]))", repo.appendingPathComponent("skills/kit/references/step_run.py").path, file.path]
        let output = Pipe()
        process.standardOutput = output
        try process.run()
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        #expect(process.terminationStatus == 0)
        let rows = try #require(JSONSerialization.jsonObject(with: data) as? [[Any]])
        for (index, row) in rows.enumerated() {
            #expect(row[0] as? String == value.steps[index].stem)
            #expect(row[2] as? String == value.steps[index].holds)
            let stems = value.steps[index].needs.compactMap { id in value.steps.first { $0.id == id }?.stem }
            #expect(row[3] as? [String] == stems)
        }
    }
}
