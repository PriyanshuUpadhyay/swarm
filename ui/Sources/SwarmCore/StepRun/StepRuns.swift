import CryptoKit
import Foundation

// A Swift copy of the step-run rules that the agent kit owns (`references/step-run.md` and
// `step_run.py`), because a fresh Mac has no kit (ADR 0046). A change there needs a matching change
// and fixture here.

/// Line 1 of a step file. An unknown word stays as text, so a new kit status shows instead of failing.
public enum StepState: Equatable, Sendable {
    case open, active(agent: String), waiting(question: String), blocked(reason: String)
    case done(revision: String), skipped(reason: String), unavailable(tool: String)
    case other(word: String, rest: String)
}

/// Checked and total items of the step's own todo list, never the skill text quoted above it.
public struct StepTodo: Equatable, Sendable {
    public let checked: Int
    public let total: Int
}

/// What a node or a run most needs from the owner, least to most urgent.
public enum StepUrgency: Int, Comparable, Sendable {
    case done, open, active, stale, blocked, waiting
    public static func < (a: Self, b: Self) -> Bool { a.rawValue < b.rawValue }
}

public struct StepNode: Identifiable, Equatable, Sendable {
    /// The file name without `.md`, such as `03-contracts`.
    public let id: String
    /// Workspace-relative, for the read-only preview.
    public let path: String
    /// Nil only with an error.
    public let state: StepState?
    public let error: String?
    /// From the `Uses:` line, or the previous step when the line is empty (`needsAssumed`).
    public let needs: [String]
    public let needsAssumed: Bool
    /// Needs whose revision changed after this step took them.
    public let stale: [String]
    public let ready: Bool
    /// Nil when the file has no todo heading.
    public let todo: StepTodo?
    public let lastEvent: Date?

    public var urgency: StepUrgency {
        switch state {
        case .waiting: .waiting
        case .blocked, .unavailable, nil: .blocked
        case _ where !stale.isEmpty: .stale
        case .active: .active
        case .done, .skipped: .done
        case .open, .other: .open
        }
    }
}

public struct StepRun: Identifiable, Equatable, Sendable {
    /// Workspace-relative folder, such as `tmp/flow/2026-10-05-login`.
    public let id: String
    public let skill: String
    public let name: String
    public let closed: Bool
    public let steps: [StepNode]
    /// The newest `events.log` time.
    public let lastActivity: Date?

    public var urgency: StepUrgency { steps.map(\.urgency).max() ?? .done }
    public var doneCount: Int {
        steps.count { if case .done = $0.state { true } else if case .skipped = $0.state { true } else { false } }
    }
    public var firstQuestion: String? {
        steps.lazy.compactMap { if case .waiting(let question) = $0.state { question } else { nil } }.first
    }
}

public enum StepRuns {
    static let todoHead = "## Todo (check a box only with its evidence after the colon; `done` refuses an empty one)"

    /// The runs under `<workspace>/tmp/<skill>/<run>/`, and `_closed/<run>/` when asked, newest first.
    /// No `tmp/` is no runs; a `tmp/` that cannot be listed throws. A run that cannot be listed drops out.
    public static func scan(workspace: String, includeClosed: Bool) async throws -> [StepRun] {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: workspace + "/tmp", isDirectory: &isDirectory) else { return [] }
        var head: String??
        var runs: [StepRun] = []
        for skill in try await WorkspaceFiles.list(in: workspace, path: "tmp").entries where skill.kind == .directory {
            guard let listing = try? await WorkspaceFiles.list(in: workspace, path: skill.path) else { continue }
            var folders = listing.entries.filter { $0.kind == .directory && $0.name != "_closed" }
            if includeClosed, listing.entries.contains(where: { $0.name == "_closed" && $0.kind == .directory }),
               let closed = try? await WorkspaceFiles.list(in: workspace, path: skill.path + "/_closed") {
                folders += closed.entries.filter { $0.kind == .directory }
            }
            for folder in folders {
                if let run = try? await read(workspace: workspace, skill: skill.name, folder: folder, head: &head) {
                    runs.append(run)
                }
            }
        }
        return runs.sorted {
            ($0.lastActivity ?? .distantPast, $0.name) > ($1.lastActivity ?? .distantPast, $1.name)
        }
    }

    /// Longest-path layers in file order: a step with no needs is layer 0, else one more than its
    /// highest need. An edge to a missing step, or one that closes a cycle, is dropped.
    public static func layers(_ steps: [StepNode]) -> [[String]] {
        let byID = Dictionary(steps.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        var layer: [String: Int] = [:]
        var visiting: Set<String> = []
        func depth(_ id: String) -> Int {
            if let known = layer[id] { return known }
            visiting.insert(id)
            let needs = (byID[id]?.needs ?? []).filter { byID[$0] != nil && !visiting.contains($0) }
            let value = needs.map { depth($0) + 1 }.max() ?? 0
            visiting.remove(id)
            layer[id] = value
            return value
        }
        // From the last step back, so the edge dropped in a cycle is the one that points to a later file.
        for step in steps.reversed() { _ = depth(step.id) }
        var result: [[String]] = []
        for step in steps {
            let value = layer[step.id] ?? 0
            while result.count <= value { result.append([]) }
            result[value].append(step.id)
        }
        return result
    }

    private struct Parsed {
        var state: StepState?
        var error: String?
        var uses: [(name: String, revision: String?)] = []
        var usesNone = false
        var revisionIsHead = false
        var todo: StepTodo?
        var hash = ""
    }

    private static func read(
        workspace: String, skill: String, folder: WorkspaceFileEntry, head: inout String??
    ) async throws -> StepRun? {
        let names = try await WorkspaceFiles.list(in: workspace, path: folder.path).entries
            .filter { $0.kind == .file && $0.name.wholeMatch(of: /[0-9]{2}-.+\.md/) != nil }
            .map(\.name).sorted()
        var parsed: [(id: String, file: Parsed)] = []
        for name in names {
            let path = folder.path + "/" + name
            var preview = try? WorkspaceFiles.read(in: workspace, path: path)
            if preview == .text("") {
                // An agent's write truncates first, so a tick can land on an empty file; read once more.
                try await Task.sleep(for: .milliseconds(150))
                preview = try? WorkspaceFiles.read(in: workspace, path: path)
            }
            parsed.append((String(name.dropLast(3)), parse(preview)))
        }
        guard parsed.contains(where: { $0.file.state != nil }) else { return nil }

        let events = events(try? WorkspaceFiles.read(in: workspace, path: folder.path + "/events.log"))
        let byID = Dictionary(parsed.map { ($0.id, $0.file) }, uniquingKeysWith: { first, _ in first })
        var steps: [StepNode] = []
        for (index, (id, file)) in parsed.enumerated() {
            let named = file.uses.map(\.name)
            let assumed = named.isEmpty && !file.usesNone && index > 0 && file.state != nil
            let needs = assumed ? [parsed[index - 1].id] : named
            var stale: [String] = []
            if case .active = file.state { stale = try await staleNeeds(file, byID, workspace, &head) }
            if case .done = file.state { stale = try await staleNeeds(file, byID, workspace, &head) }
            let ready = file.state == .open && needs.allSatisfy { need in
                switch byID[need]?.state { case .done, .skipped: true; default: false }
            }
            steps.append(StepNode(
                id: id, path: folder.path + "/" + id + ".md", state: file.state, error: file.error,
                needs: needs, needsAssumed: assumed, stale: stale, ready: ready, todo: file.todo,
                lastEvent: events[id]
            ))
        }
        let closed = folder.path.hasPrefix("tmp/\(skill)/_closed/")
        return StepRun(
            id: folder.path, skill: skill, name: folder.name, closed: closed, steps: steps,
            lastActivity: events.values.max()
        )
    }

    private static func staleNeeds(
        _ file: Parsed, _ byID: [String: Parsed], _ workspace: String, _ head: inout String??
    ) async throws -> [String] {
        var stale: [String] = []
        for use in file.uses {
            guard let used = use.revision, !used.isEmpty, let need = byID[use.name], need.state != nil else { continue }
            if need.revisionIsHead {
                if head == nil {
                    let result = try? await Git.runRaw(["rev-parse", "HEAD"], in: workspace)
                    head = .some(result.flatMap { $0.ok ? String(decoding: $0.stdout, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines) : nil })
                }
                // No HEAD answer is no judgment, not a stale mark.
                if let current = head ?? nil, !current.hasPrefix(used) { stale.append(use.name) }
            } else if need.hash != used {
                stale.append(use.name)
            }
        }
        return stale
    }

    private static func parse(_ preview: WorkspaceFilePreview?) -> Parsed {
        var file = Parsed()
        guard case .text(let text) = preview else {
            if case .notice(let notice) = preview { file.error = notice } else { file.error = "The file cannot be read" }
            return file
        }
        let rest = text.firstIndex(of: "\n").map { String(text[text.index(after: $0)...]) } ?? ""
        file.hash = Insecure.SHA1.hash(data: Data(rest.utf8)).map { String(format: "%02x", $0) }.joined().prefix(12).description
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        let header = lines.prefix { !$0.isEmpty }
        if header.count > 1, header[1].hasPrefix("Uses:") {
            let value = header[1].dropFirst(5).trimmingCharacters(in: .whitespaces)
            file.usesNone = value == "none"
            file.uses = value.split(separator: ",").compactMap { entry in
                let parts = entry.trimmingCharacters(in: .whitespaces).split(separator: "@", maxSplits: 1)
                guard let name = parts.first, name != "none" else { return nil }
                return (String(name), parts.count > 1 ? String(parts[1]) : nil)
            }
        }
        file.revisionIsHead = header.dropFirst().contains("Revision: HEAD")
        if let todo = text.range(of: todoHead) {
            let body = text[todo.upperBound...].components(separatedBy: "## Result")[0]
            let items = body.split(separator: "\n")
            let checked = items.count { $0.hasPrefix("- [x]") }
            file.todo = StepTodo(checked: checked, total: checked + items.count { $0.hasPrefix("- [ ]") })
        }
        guard let first = lines.first, first.hasPrefix("Status: ") else {
            file.error = "Line 1 is not a status"
            return file
        }
        let status = first.dropFirst(8)
        let word = String(status.prefix { $0 != " " })
        let rest1 = status.dropFirst(word.count).trimmingCharacters(in: .whitespaces)
        switch word {
        case "open": file.state = .open
        case "active": file.state = .active(agent: rest1)
        case "waiting": file.state = .waiting(question: rest1)
        case "blocked": file.state = .blocked(reason: rest1)
        case "done": file.state = .done(revision: rest1)
        case "skipped": file.state = .skipped(reason: rest1)
        case "unavailable": file.state = .unavailable(tool: rest1)
        case "": file.error = "Line 1 has no status word"
        default: file.state = .other(word: word, rest: rest1)
        }
        return file
    }

    /// The newest time per step from `events.log` lines `<local ISO time>\t<step>\t<event>`; a bad line is skipped.
    private static func events(_ preview: WorkspaceFilePreview?) -> [String: Date] {
        guard case .text(let text) = preview else { return [:] }
        let format = Date.ISO8601FormatStyle(timeZone: .current).year().month().day().time(includingFractionalSeconds: false)
        var newest: [String: Date] = [:]
        for line in text.split(separator: "\n") {
            let fields = line.split(separator: "\t", maxSplits: 2)
            guard fields.count == 3, let date = try? format.parse(String(fields[0])) else { continue }
            newest[String(fields[1])] = max(newest[String(fields[1])] ?? date, date)
        }
        return newest
    }
}
