import Foundation

public enum SkillDraftIssue: Error, Equatable, Sendable, LocalizedError {
    case source(reason: String)
    case structuralLock
    case missingStep(id: String)
    case invalidName(id: String, name: String)
    case duplicateName(name: String)
    case rowCount
    case invalidHolds(id: String)
    case invalidBody(id: String)
    case missingSection(id: String)
    case unknownNeed(id: String, need: String)
    case selfNeed(id: String)
    case duplicateNeed(id: String)
    case cycle
    case ambiguousNeeds(id: String, word: String)
    case dependentSteps(id: String, dependents: [String])
    case sectionRemovalConfirmation(id: String)
    case invalidPosition
    case renderMismatch

    public var errorDescription: String? {
        switch self {
        case .source(let reason): reason
        case .structuralLock: "this skill's script owns its step ids"
        case .missingStep(let id): "Step \(id) does not exist."
        case .invalidName(_, let name): "Invalid step name \(name). Use lowercase words with single hyphens."
        case .duplicateName(let name): "Step name \(name) is already used."
        case .rowCount: "A skill needs 1-99 steps."
        case .invalidHolds(let id): "Holds for \(id) must be one line without pipes or control characters."
        case .invalidBody(let id): "Step text for \(id) cannot add a level-one or level-two heading outside a fence."
        case .missingSection: "no step section in SKILL.md"
        case .unknownNeed(let id, let need): "Needs for \(id) refers to missing step \(need)."
        case .selfNeed(let id): "Step \(id) cannot need itself."
        case .duplicateNeed(let id): "Needs for \(id) has a duplicate step."
        case .cycle: "Needs contains a cycle."
        case .ambiguousNeeds(let id, let word): "Needs word \(word) for \(id) does not resolve to exactly one intended step."
        case .dependentSteps(let id, let dependents): "Cannot remove \(id) while \(dependents.joined(separator: ", ")) needs it."
        case .sectionRemovalConfirmation(let id): "Confirm removal of \(id) and its step section."
        case .invalidPosition: "The row position is outside the table."
        case .renderMismatch: "The saved candidate does not match the draft."
        }
    }
}

public struct SkillDraftStep: Identifiable, Equatable, Sendable {
    public let id: String
    public internal(set) var stem: String
    public internal(set) var name: String
    public internal(set) var needs: [String]
    public internal(set) var needsText: String
    public internal(set) var holds: String
    public internal(set) var body: String?
}

public struct SkillDraft: Sendable {
    public let document: SkillDocument
    public private(set) var steps: [SkillDraftStep]
    private let originalSteps: [SkillDraftStep]

    public init(document: SkillDocument) {
        self.document = document
        let values = document.rows.map { row in
            SkillDraftStep(id: row.id, stem: row.stem, name: row.name, needs: row.needs,
                           needsText: row.needsText, holds: row.holds, body: document.sections[row.id]?.body)
        }
        steps = values
        originalSteps = values
    }

    public var isChanged: Bool { steps != originalSteps }

    @discardableResult
    public mutating func add(name: String, body: String? = nil) throws -> String {
        try requireStructure()
        let id = UUID().uuidString
        let normalized = body.map { $0.normalizedLineEndings(ending: document.lineEnding) }
        try change {
            $0.steps.append(SkillDraftStep(id: id, stem: "", name: name, needs: [], needsText: "none", holds: "",
                                          body: normalized))
            $0.renumber()
        }
        return id
    }

    public mutating func remove(id: String, removeSection: Bool = false) throws {
        try requireStructure()
        let index = try index(of: id)
        let dependents = steps.filter { $0.needs.contains(id) }.map(\.stem)
        guard dependents.isEmpty else { throw SkillDraftIssue.dependentSteps(id: id, dependents: dependents) }
        guard steps[index].body == nil || removeSection else { throw SkillDraftIssue.sectionRemovalConfirmation(id: id) }
        try change { $0.steps.remove(at: index); $0.renumber() }
    }

    public mutating func rename(id: String, name: String) throws {
        try requireStructure()
        let index = try index(of: id)
        guard steps[index].name != name else { return }
        try change { $0.steps[index].name = name; $0.renumber() }
    }

    public mutating func move(id: String, to position: Int) throws {
        try requireStructure()
        let index = try index(of: id)
        guard steps.indices.contains(position) else { throw SkillDraftIssue.invalidPosition }
        guard index != position else { return }
        try change {
            let step = $0.steps.remove(at: index)
            $0.steps.insert(step, at: position)
            $0.renumber()
        }
    }

    public mutating func setNeeds(id: String, needs: [String]) throws {
        let index = try index(of: id)
        try change { $0.steps[index].needs = needs }
    }

    public mutating func setHolds(id: String, holds: String) throws {
        let index = try index(of: id)
        try change { $0.steps[index].holds = holds }
    }

    public mutating func setBody(id: String, body: String) throws {
        let index = try index(of: id)
        guard steps[index].body != nil else { throw SkillDraftIssue.missingSection(id: id) }
        let normalized = body.normalizedLineEndings(ending: document.lineEnding)
        try change { $0.steps[index].body = normalized }
    }

    public func validate() -> [SkillDraftIssue] {
        var issues: [SkillDraftIssue] = []
        if document.state != .checkoutEditable {
            issues.append(.source(reason: document.state.reason ?? "This source cannot be edited."))
        }
        if !(1...99).contains(steps.count) { issues.append(.rowCount) }
        var names = Set<String>()
        var stems = Set<String>()
        let ids = Set(steps.map(\.id))
        let targets = steps.map { (id: $0.id, name: $0.name) }
        for step in steps {
            if step.name.range(of: #"^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$"#, options: .regularExpression) == nil
                || step.name == "none" || step.name == "and" {
                issues.append(.invalidName(id: step.id, name: step.name))
            }
            if !names.insert(step.name).inserted || !stems.insert(step.stem).inserted { issues.append(.duplicateName(name: step.name)) }
            if step.holds.contains("|") || step.holds.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) || $0 == "\u{2028}" || $0 == "\u{2029}" }) {
                issues.append(.invalidHolds(id: step.id))
            }
            if let body = step.body {
                let lines = SkillLine.scan(Data(body.utf8), frontMatter: false)
                if lines.contains(where: { ($0.heading?.level ?? 7) <= 2 }) {
                    issues.append(.invalidBody(id: step.id))
                }
                // A fence that stays open would absorb the next section after splicing.
                if SkillLine.scan(Data((body + "\n## boundary\n").utf8), frontMatter: false).last?.inFence == true {
                    issues.append(.invalidBody(id: step.id))
                }
            }
            for need in step.needs where !ids.contains(need) { issues.append(.unknownNeed(id: step.id, need: need)) }
            if step.needs.contains(step.id) { issues.append(.selfNeed(id: step.id)) }
            if Set(step.needs).count != step.needs.count { issues.append(.duplicateNeed(id: step.id)) }
            let resolved = SkillNeeds.resolve(step.needsText, names: targets)
            if !resolved.errors.isEmpty || resolved.ids != step.needs {
                issues.append(.ambiguousNeeds(id: step.id, word: step.needsText))
            }
        }
        if SkillNeeds.hasCycle(steps.map { ($0.id, $0.needs) }) { issues.append(.cycle) }
        return issues
    }

    public func render() throws -> Data? {
        guard isChanged else { return nil }
        if let issue = validate().first { throw issue }
        guard let table = document.tableRange else { throw SkillDraftIssue.source(reason: "no step table") }
        var edits: [(range: Range<Int>, bytes: Data)] = []
        let tableChanged = steps.map { [$0.id, $0.stem, $0.needsText, $0.holds] }
            != originalSteps.map { [$0.id, $0.stem, $0.needsText, $0.holds] }
        if tableChanged {
            let ending = document.lineEnding
            var text = "| File | Needs | Holds |\(ending)|---|---|---|\(ending)"
            for (index, step) in steps.enumerated() {
                if let original = originalSteps.first(where: { $0.id == step.id }),
                   let row = document.rows.first(where: { $0.id == step.id }),
                   step.stem == original.stem, step.needsText == original.needsText, step.holds == original.holds {
                    text += String(decoding: document.bytes.subdata(in: row.range), as: UTF8.self)
                } else {
                    text += "| `\(step.stem).md` | \(step.needsText) | \(step.holds) |\(ending)"
                }
                if index < steps.count - 1 && text.utf8.last != 10 { text += ending }
            }
            edits.append((table, Data(text.utf8)))
        }
        for original in originalSteps {
            guard let section = document.sections[original.id] else { continue }
            guard let step = steps.first(where: { $0.id == original.id }) else {
                edits.append((section.range, Data()))
                continue
            }
            if step.stem != original.stem {
                edits.append((section.headingRange, Data("## \(step.stem)\(document.lineEnding)".utf8)))
            }
            if step.body != original.body, let body = step.body {
                let headingNeedsEnding = step.stem == original.stem && !body.isEmpty
                    && document.bytes[section.headingRange.upperBound - 1] != 10
                let prefix = headingNeedsEnding ? document.lineEnding : ""
                let separator = section.bodyRange.upperBound < document.bytes.count && !body.isEmpty && body.utf8.last != 10 ? document.lineEnding : ""
                edits.append((section.bodyRange, Data((prefix + body + separator).utf8)))
            }
        }
        let added = steps.filter { step in !originalSteps.contains(where: { $0.id == step.id }) && step.body != nil }
        if !added.isEmpty {
            let ending = document.lineEnding
            var text = document.bytes.last == 10 || document.bytes.isEmpty ? "" : ending
            for step in added {
                text += ending + "## \(step.stem)" + ending + (step.body ?? "")
                if text.utf8.last != 10 { text += ending }
            }
            edits.append((document.bytes.count..<document.bytes.count, Data(text.utf8)))
        }
        let ascending = edits.sorted { $0.range.lowerBound < $1.range.lowerBound }
        for index in ascending.indices.dropFirst() where ascending[index - 1].range.upperBound > ascending[index].range.lowerBound {
            throw SkillDraftIssue.renderMismatch
        }
        var candidate = document.bytes
        for edit in ascending.reversed() { candidate.replaceSubrange(edit.range, with: edit.bytes) }
        try verify(candidate, edits: ascending)
        return candidate == document.bytes ? nil : candidate
    }

    private func verify(_ candidate: Data, edits: [(range: Range<Int>, bytes: Data)]) throws {
        let parsed = SkillDocument.parse(data: candidate, key: document.key)
        guard parsed.state == .checkoutEditable, parsed.rows.count == steps.count else { throw SkillDraftIssue.renderMismatch }
        for (row, step) in zip(parsed.rows, steps) {
            let needs = step.needs.compactMap { id in steps.first(where: { $0.id == id })?.stem }
            guard row.stem == step.stem, row.name == step.name, row.needs == needs, row.holds == step.holds,
                  (parsed.sections[row.id] != nil) == (step.body != nil) else { throw SkillDraftIssue.renderMismatch }
            if let body = step.body, let actual = parsed.sections[row.id]?.body {
                guard actual.hasPrefix(body), actual.dropFirst(body.count).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                    throw SkillDraftIssue.renderMismatch
                }
            }
        }
        var originalOffset = 0
        var candidateOffset = 0
        for edit in edits {
            let count = edit.range.lowerBound - originalOffset
            guard document.bytes.subdata(in: originalOffset..<edit.range.lowerBound)
                    == candidate.subdata(in: candidateOffset..<(candidateOffset + count)) else { throw SkillDraftIssue.renderMismatch }
            originalOffset = edit.range.upperBound
            candidateOffset += count + edit.bytes.count
        }
        guard document.bytes.suffix(from: originalOffset) == candidate.suffix(from: candidateOffset) else { throw SkillDraftIssue.renderMismatch }
    }

    private func requireStructure() throws {
        guard document.capability == .generic else { throw SkillDraftIssue.structuralLock }
    }

    private func index(of id: String) throws -> Int {
        guard let index = steps.firstIndex(where: { $0.id == id }) else { throw SkillDraftIssue.missingStep(id: id) }
        return index
    }

    private mutating func change(_ update: (inout SkillDraft) -> Void) throws {
        var candidate = self
        update(&candidate)
        candidate.refreshNeedsText()
        if let issue = candidate.validate().first { throw issue }
        self = candidate
    }

    private mutating func renumber() {
        for index in steps.indices { steps[index].stem = String(format: "%02d-%@", index + 1, steps[index].name) }
    }

    private mutating func refreshNeedsText() {
        for index in steps.indices {
            let step = steps[index]
            guard let original = originalSteps.first(where: { $0.id == step.id }), original.needs == step.needs else {
                steps[index].needsText = step.needs.isEmpty ? "none" : step.needs.compactMap { id in steps.first(where: { $0.id == id })?.name }.joined(separator: ", ")
                continue
            }
            var text = original.needsText as NSString
            for token in SkillNeeds.words(original.needsText).reversed() {
                let matches = originalSteps.filter { $0.name.hasPrefix(token.word) }
                guard matches.count == 1, let current = steps.first(where: { $0.id == matches[0].id }), current.name != matches[0].name else { continue }
                text = text.replacingCharacters(in: token.range, with: current.name) as NSString
            }
            steps[index].needsText = text as String
        }
    }
}

private extension String {
    func normalizedLineEndings(ending: String) -> String {
        replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
            .replacingOccurrences(of: "\n", with: ending)
    }
}
