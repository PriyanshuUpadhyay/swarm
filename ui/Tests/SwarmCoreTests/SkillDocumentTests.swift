import Foundation
import Testing
@testable import SwarmCore

@Suite("Skill document")
struct SkillDocumentTests {
    static let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .appendingPathComponent("Fixtures/Skills")

    static func fixture(_ name: String) throws -> Data {
        try Data(contentsOf: root.appendingPathComponent(name + ".md"))
    }

    @Test("The four kit tables have their real rows and capabilities")
    func kitTables() throws {
        for (role, count, sectionCount, generic) in [
            ("research-like", 6, 6, true), ("flow-like", 7, 0, false),
            ("council-like", 5, 0, true), ("web-search-like", 5, 0, true),
        ] {
            let document = SkillDocument.parse(data: try Self.fixture(role), key: "kit/skills/\(role)/SKILL.md")
            #expect(document.rows.count == count)
            #expect(document.sections.count == sectionCount)
            #expect(document.state == .checkoutEditable)
            #expect((document.capability == .generic) == generic)
            #expect(document.revision.count == 64)
        }
        let flow = SkillDocument.parse(data: try Self.fixture("flow-like"), key: "flow")
        #expect(flow.rows[4].needs == ["04-impact", "02-design"])
        #expect(flow.rows[6].needsText == "review with APPROVE")
        #expect(flow.capability.reason == "this skill's script owns its step ids")
    }

    @Test("Ranges are UTF-8 byte ranges and use the first table only")
    func byteRanges() throws {
        for role in ["multibyte", "crlf", "two-tables", "fenced-heading"] {
            let data = try Self.fixture(role)
            let document = SkillDocument.parse(data: data, key: role)
            #expect(document.state == .checkoutEditable)
            #expect(document.rows.count == 2)
            #expect(document.sections.count == 2)
            let section = try #require(document.sections["01-question"])
            let body = String(decoding: data.subdata(in: section.bodyRange), as: UTF8.self)
            #expect(body == section.body)
            #expect(body.contains("Question α"))
            #expect(body.contains("### Detail"))
            #expect(!body.contains("## Guards"))
            let table = try #require(document.tableRange)
            #expect(String(decoding: data.subdata(in: table), as: UTF8.self).contains("café 🚀"))
            #expect(document.lineEnding == (role == "crlf" ? "\r\n" : "\n"))
        }
    }

    @Test("No table and unsafe source shapes have explicit states")
    func shapes() throws {
        #expect(SkillDocument.parse(data: try Self.fixture("no-table"), key: "plain").state == .noTable)
        let table = "| File | Needs | Holds |\n|---|---|---|\n| `01-question.md` | none | body |\n"
        for text in ["---\n" + table + "---\n", "```\n" + table + "```\n", "~~~\n" + table + "~~~\n",
                     table.replacingOccurrences(of: "|---|---|---|", with: "| --- | --- | --- |"),
                     table.replacingOccurrences(of: "body |", with: "body | extra |"),
                     table.replacingOccurrences(of: "01-question", with: "1-question"),
                     table.replacingOccurrences(of: "none", with: "missing")] {
            let document = SkillDocument.parse(data: Data(text.utf8), key: "shape")
            if case .invalidSource(let reason) = document.state { #expect(!reason.isEmpty) }
            else { Issue.record("Expected invalid source for \(text)") }
        }
        let duplicate = SkillDocument.parse(data: try Self.fixture("duplicate-heading"), key: "duplicate")
        if case .invalidSource(let reason) = duplicate.state { #expect(reason.contains("heading")) }
        else { Issue.record("Duplicate heading was accepted") }
    }

    @Test("Unknown, ambiguous, duplicate, self and cyclic Needs fail")
    func needs() {
        let prefix = "| File | Needs | Holds |\n|---|---|---|\n"
        for rows in [
            "| 01-report.md | none | |\n| 02-report-extra.md | report | |\n",
            "| 01-first.md | first | |\n",
            "| 01-first.md | second | |\n| 02-second.md | first | |\n",
            "| 01-first.md | none | |\n| 02-second.md | first, first | |\n",
            "| 01-first.md | none | |\n| 01-second.md | none | |\n",
        ] {
            let document = SkillDocument.parse(data: Data((prefix + rows).utf8), key: "needs")
            if case .invalidSource = document.state {} else { Issue.record("Accepted unsafe Needs or prefixes") }
        }
    }

    @Test("The kit parser is an independent witness for every table fixture")
    func pythonParity() throws {
        let repo = Self.root.deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let script = repo.appendingPathComponent("skills/kit/references/step_run.py")
        for role in ["research-like", "flow-like", "council-like", "web-search-like", "multibyte", "crlf", "two-tables", "fenced-heading", "duplicate-heading", "no-table"] {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
            process.arguments = ["-I", "-c", "import runpy,json,sys\nm=runpy.run_path(sys.argv[1])\ntry:\n rows=m['table'](sys.argv[2])\nexcept SystemExit:\n rows=[]\nprint(json.dumps(rows))", script.path, Self.root.appendingPathComponent(role + ".md").path]
            let output = Pipe()
            process.standardOutput = output
            try process.run()
            let data = output.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            #expect(process.terminationStatus == 0)
            let rows = try JSONDecoder().decode([[String]].self, from: data)
            let document = SkillDocument.parse(data: try Self.fixture(role), key: role)
            #expect(rows == document.rows.map { [$0.stem, $0.needsText, $0.holds] })
        }
    }

    @Test("Inventory lists all manifest SKILL.md files and qualifies repeated names")
    func inventory() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let keys = ["kit/skills/research/SKILL.md", "kit/vendor/vendor/skills/research/SKILL.md", "swarm-voice/SKILL.md"]
        for key in keys {
            let url = folder.appendingPathComponent(key)
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Self.fixture(key.contains("vendor") ? "no-table" : "research-like").write(to: url)
        }
        let manifest = try JSONSerialization.data(withJSONObject: ["schema": 1, "files": (keys + ["kit/README.md"]).map { ["path": $0] }])
        let inventory = try SkillInventory(manifest: manifest, root: folder)
        #expect(inventory.entries.count == 3)
        #expect(inventory.entries.filter { $0.name == "research" }.allSatisfy { $0.qualifier != nil })
        #expect(inventory.entries.filter(\.hasStepTable).count == 2)
    }
}
