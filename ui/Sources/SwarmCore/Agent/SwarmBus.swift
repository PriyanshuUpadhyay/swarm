import Foundation

// The agents Swarm starts through swarm, and the messages it exchanges with them.
//
// Swarm is the chair of every agent it starts (docs/decisions/0007 at the root of the swarm
// repository). `src/bus.rs` and `src/main.rs` own the commands and JSON, so a shape change must
// update swarm's output and these types together. The JSON keys are snake_case; decode with
// `.convertFromSnakeCase`.

/// One agent in a swarm session. `pane` is nil for an agent that was registered and
/// never spawned, and for one that was closed or reported dead; `alive` is nil whenever `pane` is,
/// or when swarm could not list panes. The chair has a pane: `agent add` records the pane its own
/// adapter reports and refuses an empty answer, which is what a send is delivered to.
public struct SwarmAgent: Sendable, Hashable, Codable, Identifiable {
    public var id: SwarmAgentID
    public var role: String
    public var provider: String?
    public var pane: String?
    public var alive: Bool?
    public var createdAt: Int?
    /// working, waiting, done, failed, or null. Text, so a new value still decodes; views read
    /// `status`.
    public var state: String?
    public var stateAtS: Int?
    /// hook or screen.
    public var stateSource: String?
    public var stateDetail: String?
    /// The chat log the agent's provider hooks reported, or nil before the first report.
    public var log: String?
    /// The question the agent's screen shows now; `swarm answer` picks one of its choices.
    public var prompt: SwarmPrompt?

    public init(
        id: SwarmAgentID, role: String, pane: String?, alive: Bool?,
        provider: String? = nil, createdAt: Int? = nil, state: String? = nil
    ) {
        self.id = id
        self.role = role
        self.provider = provider
        self.pane = pane
        self.alive = alive
        self.createdAt = createdAt
        self.state = state
    }
}

/// A question on an agent's screen, as `swarm agents --json` reads it (ADR 0029).
public struct SwarmPrompt: Sendable, Hashable, Codable, Identifiable {
    /// A hash of the question and choices; `swarm answer` refuses a stale one.
    public var id: String
    public var question: String
    /// The CLI's own labels, in screen order.
    public var choices: [String]

    public init(id: String, question: String, choices: [String]) {
        self.id = id
        self.question = question
        self.choices = choices
    }
}

/// `swarm setup status --json`: which setup groups have nothing pending (ADR 0043).
public struct SwarmSetupStatus: Sendable, Hashable, Codable {
    public var hooks: Bool
    public var trust: Bool
    public var herdr: Bool

    public init(hooks: Bool, trust: Bool, herdr: Bool) {
        self.hooks = hooks
        self.trust = trust
        self.herdr = herdr
    }

    /// Whether the app asks on start. A hooks "Not now", or a group the owner left unchecked when
    /// they applied the rest, covers only that group, so a Mac that updates still sees the sheet
    /// once for folder trust (Q3).
    public func needsSheet(hooksDeclined: Bool, trustDeclined: Bool) -> Bool {
        (!trust && !trustDeclined) || !herdr || (!hooks && !hooksDeclined)
    }
}

/// What the owner picked in the setup sheet: the groups to apply and the launch consent
/// (02-design screen 1). A group left out is not planned, so the digest covers what the sheet
/// shows.
public struct SwarmSetupChoice: Sendable, Hashable {
    /// Each group the full plan showed, in its order.
    public var groups: [String]
    /// The groups whose checkbox the owner cleared.
    public var unchecked: Set<String>
    /// The launch consent the trust group writes: standing, or ask in the agent's column; nil
    /// until the first plan reports the owner's recorded answer.
    public var standing: Bool?

    public init(groups: [String] = [], unchecked: Set<String> = [], standing: Bool? = nil) {
        self.groups = groups
        self.unchecked = unchecked
        self.standing = standing
    }

    /// The groups to apply.
    public var checked: [String] { groups.filter { !unchecked.contains($0) } }

    /// The `swarm setup` flags of this choice; none for every group with the recorded consent.
    public var arguments: [String] {
        var arguments: [String] = []
        if !unchecked.isEmpty { arguments += ["--only", checked.joined(separator: ",")] }
        if let standing { arguments += ["--consent", standing ? "standing" : "ask"] }
        return arguments
    }

    /// Takes what a plan shows: each new group gets a checkbox, so no planned file goes unseen,
    /// and the radio starts at the answer the first plan sets, the owner's recorded one.
    public mutating func take(_ plan: SwarmHooksPlan) {
        for group in plan.groupIDs where !groups.contains(group) {
            groups.append(group)
        }
        if standing == nil, let consent = plan.consent {
            standing = consent == "standing"
        }
    }
}

/// `swarm hooks setup --plan --json`: each file that setup would change, with its unified diff,
/// and each entry of the owner's at a place where swarm needs its own (ADR 0036). Apply takes
/// `digest` back and refuses a file that changed after the owner saw this plan.
public struct SwarmHooksPlan: Sendable, Hashable, Codable {
    public struct File: Sendable, Hashable, Codable, Identifiable {
        public var path: String
        public var diff: String
        /// The `swarm setup` group, such as `hooks` or `trust`; nil from `hooks setup`. The list
        /// may grow, so the sheet shows a group it does not know by its name.
        public var group: String?

        public init(path: String, diff: String, group: String? = nil) {
            self.path = path
            self.diff = diff
            self.group = group
        }

        /// Group and path, because hooks and trust can both change one file in one plan.
        public var id: String { (group ?? "") + "\u{0}" + path }
        public var added: Int { count("+") }

        /// The diff with the `diff --git` line that the app's diff viewer needs to draw it as a
        /// diff, with quoted names as `TranscriptDiffPreview` writes them.
        public var patch: String {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .withoutEscapingSlashes
            func quoted(_ name: String) -> String {
                String(decoding: try! encoder.encode(name), as: UTF8.self)
            }
            return "diff --git \(quoted("a" + path)) \(quoted("b" + path))\n" + diff
        }
        public var removed: Int { count("-") }

        /// Diff lines with `mark` after the two file header lines, so an added line that starts
        /// with `++` still counts.
        private func count(_ mark: Character) -> Int {
            diff.split(separator: "\n").dropFirst(2).filter { $0.first == mark }.count
        }
    }

    public struct Conflict: Sendable, Hashable, Codable, Identifiable {
        public var file: String
        public var entry: String
        public var found: String
        public var wanted: String
        public var fix: String
        /// Open set: `taken`, `changed`, `order`, `unreadable`, and later ones; nil from an older
        /// CLI.
        public var kind: String?
        /// The `swarm setup` group; nil from `hooks setup` and an undo.
        public var group: String?

        public init(
            file: String, entry: String, found: String, wanted: String, fix: String,
            kind: String? = nil, group: String? = nil
        ) {
            self.kind = kind
            self.file = file
            self.entry = entry
            self.found = found
            self.wanted = wanted
            self.fix = fix
            self.group = group
        }

        public var id: String { file + "\u{0}" + entry }
        /// The label of `wanted`: swarm's own value only for an item that changed after swarm
        /// wrote it; for any other cause, `wanted` is what swarm needs.
        public var wantedLabel: String { kind == "changed" ? "Swarm wrote" : "Swarm needs" }
    }

    public var digest: String
    public var files: [File]
    public var conflicts: [Conflict]
    /// The launch consent a `swarm setup` plan sets; nil from `hooks setup` and an undo.
    public var consent: String?

    public init(digest: String, files: [File], conflicts: [Conflict], consent: String? = nil) {
        self.digest = digest
        self.files = files
        self.conflicts = conflicts
        self.consent = consent
    }

    /// Nothing to change and nothing in the way.
    public var isSetUp: Bool { files.isEmpty && conflicts.isEmpty }
    /// Each `swarm setup` group with a file or a conflict, in plan order.
    public var groupIDs: [String] {
        var seen: [String] = []
        for group in files.compactMap(\.group) + conflicts.compactMap(\.group)
        where !seen.contains(group) {
            seen.append(group)
        }
        return seen
    }
    /// Setup writes no file while any conflict stands.
    public var canApply: Bool { conflicts.isEmpty && !files.isEmpty }
    /// What VoiceOver hears when the plan loads.
    public var summary: String {
        if isSetUp { return "Swarm's hooks are already set up." }
        let fileCount = files.count == 1 ? "1 file" : "\(files.count) files"
        let conflictCount = conflicts.count == 1 ? "1 conflict" : "\(conflicts.count) conflicts"
        return "\(fileCount) to change, \(conflictCount)."
    }
}

public struct SwarmAgentList: Sendable, Hashable, Codable {
    public var agents: [SwarmAgent]
    public var attachable: Bool?

    public init(agents: [SwarmAgent], attachable: Bool? = nil) {
        self.agents = agents
        self.attachable = attachable
    }
}

/// One message on the bus. `body` is nil when swarm could not read the message's body file.
public struct SwarmMessage: Sendable, Hashable, Codable, Identifiable {
    public var id: Int { seq }

    public var seq: Int
    public var sender: SwarmAgentID
    public var recipient: SwarmAgentID
    public var kind: String
    public var body: String?
    public var createdAt: Int
    public var read: Bool

    public init(
        seq: Int, sender: SwarmAgentID, recipient: SwarmAgentID, kind: String,
        body: String?, createdAt: Int, read: Bool
    ) {
        self.seq = seq
        self.sender = sender
        self.recipient = recipient
        self.kind = kind
        self.body = body
        self.createdAt = createdAt
        self.read = read
    }
}

public struct SwarmMessageList: Sendable, Hashable, Codable {
    public var messages: [SwarmMessage]

    public init(messages: [SwarmMessage]) {
        self.messages = messages
    }
}

/// One swarm session discovered through the bus.
///
/// The bus writes its UUID v7 session id as JSON text.
public struct SwarmSession: Sendable, Hashable, Codable, Identifiable {
    public var id: SwarmSessionID
    public var talkMode: String
    public var adapter: String?
    public var cwd: String
    public var createdAt: Int
    public var chairProvider: String?
    public var chairID: SwarmChairID?
    public var chairLog: String?
    public var continuationOf: SwarmSessionID?
    public var agents: Int
    public var messages: Int
    public var lastMessageAt: Int?
    public var archivedAt: Int?

    public init(
        id: SwarmSessionID, talkMode: String, adapter: String?, cwd: String, createdAt: Int,
        chairProvider: String? = nil, chairID: SwarmChairID? = nil,
        chairLog: String?, agents: Int, messages: Int, lastMessageAt: Int?,
        continuationOf: SwarmSessionID? = nil,
        archivedAt: Int? = nil
    ) {
        self.id = id
        self.talkMode = talkMode
        self.adapter = adapter
        self.cwd = cwd
        self.createdAt = createdAt
        self.chairProvider = chairProvider
        self.chairID = chairID
        self.chairLog = chairLog
        self.continuationOf = continuationOf
        self.agents = agents
        self.messages = messages
        self.lastMessageAt = lastMessageAt
        self.archivedAt = archivedAt
    }

    private enum CodingKeys: String, CodingKey {
        case id, talkMode, adapter, cwd, createdAt, chairProvider, chairID = "chairId", chairLog, continuationOf
        case agents, messages, lastMessageAt, archivedAt
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = SwarmSessionID(try values.decode(String.self, forKey: .id))
        talkMode = try values.decode(String.self, forKey: .talkMode)
        adapter = try values.decodeIfPresent(String.self, forKey: .adapter)
        cwd = try values.decode(String.self, forKey: .cwd)
        createdAt = try values.decode(Int.self, forKey: .createdAt)
        chairProvider = try values.decodeIfPresent(String.self, forKey: .chairProvider)
        chairID = try values.decodeIfPresent(SwarmChairID.self, forKey: .chairID)
        chairLog = try values.decodeIfPresent(String.self, forKey: .chairLog)
        continuationOf = try values.decodeIfPresent(SwarmSessionID.self, forKey: .continuationOf)
        agents = try values.decode(Int.self, forKey: .agents)
        messages = try values.decode(Int.self, forKey: .messages)
        lastMessageAt = try values.decodeIfPresent(Int.self, forKey: .lastMessageAt)
        archivedAt = try values.decodeIfPresent(Int.self, forKey: .archivedAt)
    }

    public func encode(to encoder: any Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(id.rawValue, forKey: .id)
        try values.encode(talkMode, forKey: .talkMode)
        try values.encodeIfPresent(adapter, forKey: .adapter)
        try values.encode(cwd, forKey: .cwd)
        try values.encode(createdAt, forKey: .createdAt)
        try values.encodeIfPresent(chairProvider, forKey: .chairProvider)
        try values.encodeIfPresent(chairID, forKey: .chairID)
        try values.encodeIfPresent(chairLog, forKey: .chairLog)
        try values.encodeIfPresent(continuationOf, forKey: .continuationOf)
        try values.encode(agents, forKey: .agents)
        try values.encode(messages, forKey: .messages)
        try values.encodeIfPresent(lastMessageAt, forKey: .lastMessageAt)
        try values.encodeIfPresent(archivedAt, forKey: .archivedAt)
    }
}

public struct SwarmChair: Sendable, Hashable {
    public var provider: String
    public var id: SwarmChairID

    public init?(agent: AgentKind, id: String) {
        let provider: String? = switch agent {
        case .claudeCode: "claude"
        case .codex: "codex"
        case .cursor, .openCode, .grok: nil
        }
        guard let provider, !id.isEmpty else { return nil }
        self.provider = provider
        self.id = SwarmChairID(id)
    }

    public var argument: String { provider + ":" + id.rawValue }
}

public struct SwarmSessionList: Sendable, Hashable, Codable {
    public var sessions: [SwarmSession]

    public init(sessions: [SwarmSession]) {
        self.sessions = sessions
    }
}

/// What `swarm launch` reported: the new pane, the account it runs on when one was asked for,
/// and the model its role or pick resolved to.
public struct SwarmLaunch: Sendable, Hashable {
    public var pane: String
    public var account: String?
    public var model: String?
    /// Each folder trust entry the launch wrote, so the app shows it (ADR 0043, owner answer I1).
    public var trustWrites: [SwarmTrustWrite]

    public init(pane: String, account: String?, model: String? = nil, trustWrites: [SwarmTrustWrite] = []) {
        self.pane = pane
        self.account = account
        self.model = model
        self.trustWrites = trustWrites
    }

    /// What VoiceOver hears about the trust writes, in one announcement so none cuts off another;
    /// nil when the launch wrote none.
    public var trustAnnouncement: String? {
        guard !trustWrites.isEmpty else { return nil }
        return (trustWrites.map(\.sentence) + [SwarmTrustWrite.undo]).joined(separator: " ")
    }
}

/// A folder that `swarm launch` marked trusted for one provider, from its
/// `trusted <provider> <dir>` line.
public struct SwarmTrustWrite: Sendable, Hashable {
    public var provider: String
    public var directory: String

    public init(provider: String, directory: String) {
        self.provider = provider
        self.directory = directory
    }

    /// `<provider> <dir>`; the dir may hold spaces.
    init?(line: String) {
        let parts = line.split(separator: " ", maxSplits: 1)
        guard parts.count == 2 else { return nil }
        self.init(provider: String(parts[0]), directory: String(parts[1]))
    }

    static let undo = "Undo it in Swarm › Managed Changes."

    /// What the chat says about the write. The line is the same for standing consent and a
    /// chair's folder pick, so it names no reason.
    public var notice: String { sentence + " " + Self.undo }

    var sentence: String {
        let folder = (directory as NSString).abbreviatingWithTildeInPath
        let name = provider == "agy" ? "AGY" : provider.capitalized
        return "Swarm marked \(folder) as trusted for \(name)."
    }
}

/// The process that shows one agent's live pane: `swarm attach <agent>` with the session's
/// environment. A value, so the terminal view starts it and nothing here runs it.
/// Where Swarm starts, reads and talks to swarm agents. The live bus runs the `swarm` CLI; tests
/// and previews hand in their own. Failures are `SwarmProfileError`, the same two kinds the
/// profile source throws.
public protocol SwarmBus: Sendable {
    /// Creates the session whose chair is the app's interactive CLI. The chair registers from its
    /// tmux pane immediately before that CLI starts.
    func startChairSession(
        chair: SwarmChair?, directory: String
    ) async throws -> SwarmSessionID
    func setChair(_ chair: SwarmChair, in session: SwarmSessionID) async throws
    /// `swarm launch`, run in `directory`. `account` is `"auto"`, an account name, or nil for the
    /// CLI's default home.
    func launch(
        _ agent: SwarmAgentID, role: String, provider: String?, model: String?, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch
    func agents(in session: SwarmSessionID, adapter: String) async throws -> [SwarmAgent]
    func agentsBySession() async throws -> [SwarmSessionID: [SwarmAgent]]
    func agentListing(
        in session: SwarmSessionID, adapter: String
    ) async throws -> SwarmAgentList
    func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage]
    /// `swarm sessions --json`, with no session selected in the environment.
    func sessions() async throws -> [SwarmSession]
    /// `swarm session archive <id>...`, with no session selected in the environment.
    func archive(_ sessions: [SwarmSessionID]) async throws
    /// Save a continuation after the new chair has received its handoff message.
    func linkChat(_ newSession: SwarmSessionID, after oldSession: SwarmSessionID) async throws
    /// `swarm type <agent>` with `text` on stdin.
    func type(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws
    /// `swarm interrupt <agent>`.
    func interrupt(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws
    /// `swarm answer <agent> <prompt> <choice>`: picks a choice of the question the agent shows.
    func answer(
        _ prompt: SwarmPrompt, choice: Int, to agent: SwarmAgentID,
        in session: SwarmSessionID, adapter: String
    ) async throws
    /// `swarm key <agent> <key>`, where swarm allows only `Up` and `C-u`.
    func pressKey(
        _ key: String, agent: SwarmAgentID, session: SwarmSessionID, adapter: String
    ) async throws
    func close(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws
}

public extension SwarmBus {
    func agentsBySession() async throws -> [SwarmSessionID: [SwarmAgent]] {
        throw SwarmProfileError.unavailable("swarm agent batches are not available")
    }

    func agentListing(
        in session: SwarmSessionID, adapter: String
    ) async throws -> SwarmAgentList {
        SwarmAgentList(agents: try await agents(in: session, adapter: adapter))
    }

    func startChairSession(
        chair: SwarmChair?, directory: String
    ) async throws -> SwarmSessionID {
        throw SwarmProfileError.unavailable("swarm chair sessions are not available")
    }

    func setChair(_ chair: SwarmChair, in session: SwarmSessionID) async throws {
        throw SwarmProfileError.unavailable("swarm chair sessions are not available")
    }

    func archive(_ sessions: [SwarmSessionID]) async throws {
        throw SwarmProfileError.unavailable("swarm session archives are not available")
    }

    func answer(
        _ prompt: SwarmPrompt, choice: Int, to agent: SwarmAgentID,
        in session: SwarmSessionID, adapter: String
    ) async throws {
        throw SwarmProfileError.unavailable("swarm answers are not available")
    }

    func linkChat(_ newSession: SwarmSessionID, after oldSession: SwarmSessionID) async throws {
        throw SwarmProfileError.unavailable("swarm chat links are not available")
    }

    func pressKey(
        _ key: String, agent: SwarmAgentID, session: SwarmSessionID, adapter: String
    ) async throws {
        throw SwarmProfileError.unavailable("swarm keys are not available")
    }

    func agents(in session: SwarmSessionID) async throws -> [SwarmAgent] {
        try await agents(in: session, adapter: SwarmSessionInteraction.defaultAdapter)
    }

    func agentListing(in session: SwarmSessionID) async throws -> SwarmAgentList {
        try await agentListing(in: session, adapter: SwarmSessionInteraction.defaultAdapter)
    }

    func messages(in session: SwarmSessionID, after seq: Int) async throws -> [SwarmMessage] {
        try await messages(
            in: session, after: seq, adapter: SwarmSessionInteraction.defaultAdapter
        )
    }

    func agents(in session: SwarmSession) async throws -> [SwarmAgent] {
        try await agents(in: session.id, adapter: try SwarmSessionInteraction.adapter(for: session))
    }

    func messages(in session: SwarmSession, after seq: Int) async throws -> [SwarmMessage] {
        try await messages(
            in: session.id, after: seq, adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func type(_ text: String, to agent: SwarmAgentID, in session: SwarmSession) async throws {
        try await type(
            text, to: agent, in: session.id,
            adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func interrupt(_ agent: SwarmAgentID, in session: SwarmSession) async throws {
        try await interrupt(
            agent, in: session.id, adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func answer(
        _ prompt: SwarmPrompt, choice: Int, to agent: SwarmAgentID, in session: SwarmSession
    ) async throws {
        try await answer(
            prompt, choice: choice, to: agent, in: session.id,
            adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func pressKey(_ key: String, agent: SwarmAgentID, session: SwarmSession) async throws {
        try await pressKey(
            key, agent: agent, session: session.id,
            adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }

    func close(_ agent: SwarmAgentID, in session: SwarmSession) async throws {
        try await close(
            agent, in: session.id, adapter: try SwarmSessionInteraction.adapter(for: session)
        )
    }
}

/// The bus before swarm is connected. Every call fails as unavailable, so a view shows its empty
/// state rather than a wrong list.
public struct UnavailableSwarmBus: SwarmBus {
    public init() {}

    private var notConnected: SwarmProfileError { .unavailable("swarm is not connected") }

    public func launch(
        _ agent: SwarmAgentID, role: String, provider: String?, model: String?, account: String?,
        in session: SwarmSessionID, directory: String
    ) async throws -> SwarmLaunch {
        throw notConnected
    }

    public func agents(
        in session: SwarmSessionID, adapter: String
    ) async throws -> [SwarmAgent] { throw notConnected }

    public func messages(
        in session: SwarmSessionID, after seq: Int, adapter: String
    ) async throws -> [SwarmMessage] {
        throw notConnected
    }

    public func sessions() async throws -> [SwarmSession] { throw notConnected }

    public func type(
        _ text: String, to agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        throw notConnected
    }

    public func interrupt(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws {
        throw notConnected
    }

    public func close(
        _ agent: SwarmAgentID, in session: SwarmSessionID, adapter: String
    ) async throws { throw notConnected }

}
