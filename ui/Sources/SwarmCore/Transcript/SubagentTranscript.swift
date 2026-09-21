import Foundation

/// A subagent's own transcript, read as the rows the window already knows how to draw.
///
/// **What `output_file` turned out to be**, because this was the one thing worth checking rather
/// than assuming. `system/task_notification.output_file` is an absolute path to
/// `<cwd slug>/<session>/tasks/<task_id>.output`, which is a **symlink** into
/// `~/.claude/projects/<cwd slug>/<session>/subagents/agent-<task_id>.jsonl`. The target is NDJSON
/// in the same shape as Claude Code's own transcripts: one object per line with a `type`, and for
/// `user` and `assistant` a `message` whose `content` is either a bare string or the usual array
/// of typed blocks. It is written for a subagent that failed exactly as it is for one that
/// worked: the three failed subagents in the capture each left a four line file holding the
/// prompt, two attachment records and an assistant message carrying the API error. So there is no
/// need for the fallback to Swarm's own nested rows, and this is the honest source.
///
/// **Why it hands back `Message` values rather than a shape of its own, which is the whole of
/// this file's design.** The version before this invented an `Entry` with a body of plain text,
/// and the pane drew each one as a `Text`. That is a second renderer for a conversation, and it
/// lost every argument the first one had already won: markdown arrived as literal asterisks, a
/// Bash call arrived as pretty printed JSON with a space before the colon, and a page of tool
/// input sat where the transcript would have drawn one line. A subagent's line and a stored
/// transcript row are the same object, so this reads one into the other and the drawing is the
/// drawing the transcript already does. Nothing here is stored: these messages belong to no
/// `messages` row, and `id` says so by being negative.
///
/// **The prompt is a user turn on both sources and it arrives in two different shapes**, which is
/// the bug that put it on screen twice. In the file it is a `user` line whose `content` is a bare
/// string. On the parent's live stream it is a `user` line whose `content` is an ARRAY holding one
/// `text` block, and the reader here used to look at the block's type without looking at whose
/// message it was, so the brief was read back as something the subagent had said and drawn under
/// "Answered" as well as under "Asked". A `text` block on a `user` line is the brief, on either
/// source, and it is taken out of the rows and handed back as `prompt`.
///
/// Swarm does not own the file, which is why nothing here throws on a shape it does not know: a
/// line that will not parse is skipped, a file that will not open is a sentence in the pane, and
/// a format that changes under us degrades to fewer rows rather than to an empty pane.
public struct SubagentTranscript: Sendable, Equatable {
    /// The conversation, oldest first, as the messages a transcript is built from.
    ///
    /// Deliberately not paired up here: a `tool_result` is its own message with the call's id in
    /// `refID`, exactly as the store hands one back, so the window folds it onto its call with the
    /// same rule it uses for every other transcript and the two cannot drift apart.
    public let messages: [Message]

    /// How many messages were dropped off the front to keep the pane bounded. Drawn as a line
    /// saying so, because a transcript that silently starts in the middle is a lie about what
    /// the subagent did.
    public let droppedRows: Int

    /// The brief the subagent was handed, when the source carried one.
    ///
    /// Read back even though `task_started` already carries it, because the two sources fail at
    /// different moments: a pane opened on a turn that has since been replaced has no
    /// `task_started` left to read, and the file has the brief in it either way.
    public let prompt: String

    /// What a background command printed. Empty for an agent.
    ///
    /// Not NDJSON and not parsed: a `local_bash` task writes plain stdout to
    /// `tasks/<task_id>.output`, and the whole of what it has to say is that text.
    public let printed: String

    /// How many of `messages`, counting from the end, arrived on the read that produced this.
    ///
    /// **This is what lets a caller fold a new line onto the rows it already drew.** Equal to
    /// `messages.count` whenever what it drew cannot be trusted: a first read, a file truncated
    /// under us, and any read that dropped messages off the front to stay inside the limit. So a
    /// caller takes the last `appended` and leaves the rest alone, and a caller that rebuilds
    /// everything can ignore it. Defaults to all of them, which is the answer that is never wrong.
    public let appended: Int

    public init(
        messages: [Message] = [],
        droppedRows: Int = 0,
        prompt: String = "",
        printed: String = "",
        appended: Int? = nil
    ) {
        self.messages = messages
        self.droppedRows = droppedRows
        self.prompt = prompt
        self.printed = printed
        self.appended = appended ?? messages.count
    }

    public var isEmpty: Bool { messages.isEmpty && printed.isEmpty }

    /// How many rows the pane will draw.
    ///
    /// The LAST of them are kept rather than the first: the answer is at the end, and the brief,
    /// which is the one early row worth having, is drawn above the conversation rather than in it.
    /// Higher than the 120 entries this replaced, because a row is now built only when it is
    /// scrolled to and a tool call's payload is only decoded when it is opened, so the number no
    /// longer stands for a screenful of laid out `Text` views.
    public static let rowLimit = 500

    /// Read one file, or one run of stored stream lines, of NDJSON.
    ///
    /// `attachment` records are skipped whole. In the capture two of every four lines were one,
    /// each carrying the subagent's entire deferred tool list, thousands of characters of it, and
    /// none of it is an account of what the subagent did.
    ///
    /// - Parameter sessionID: the session these lines came off, which is the parent's. It is
    ///   carried because a `Message` has one and for no other reason: nothing drawn from these
    ///   rows reads it.
    public static func parse(_ text: String, sessionID: SessionID) -> SubagentTranscript {
        parse(text, sessionID: sessionID, userText: .brief)
    }

    /// Reads a chair transcript through the same Claude Code NDJSON parser, while keeping every
    /// user turn as a transcript row instead of lifting the one subagent brief out of the list.
    public static func parseChair(
        _ text: String, sessionID: SessionID, limit: Int? = rowLimit
    ) -> SubagentTranscript {
        parse(text, sessionID: sessionID, userText: .row, limit: limit)
    }

    private static func parse(
        _ text: String, sessionID: SessionID, userText: TranscriptMapping.UserText,
        limit: Int? = rowLimit
    ) -> SubagentTranscript {
        var messages: [Message] = []
        var prompt = ""
        var used = Set<Int64>()

        for source in text.split(whereSeparator: \.isNewline) {
            let raw = Data(source.utf8)
            guard let json = JSONValue.parse(raw) else { continue }
            if userText == .row, TranscriptMapping.claudeScaffolding(json) {
                messages.append(Message(
                    id: identifier(for: raw, avoiding: &used),
                    sessionID: sessionID,
                    seq: messages.count,
                    kind: .system,
                    payload: raw,
                    refID: nil
                ))
                continue
            }
            for reading in TranscriptMapping.claude(json, raw: raw, userText: userText) {
                switch reading {
                case .brief(let brief):
                    // The last one wins. A file holds exactly one; a run of stored stream lines
                    // holds one per subagent and this is only ever handed one subagent's.
                    prompt = brief
                case .block(let block):
                    messages.append(Message(
                        id: identifier(for: block.payload, avoiding: &used),
                        sessionID: sessionID,
                        seq: messages.count,
                        kind: block.kind,
                        payload: block.payload,
                        refID: block.refID
                    ))
                }
            }
        }

        guard let limit else {
            return SubagentTranscript(messages: messages, prompt: prompt)
        }
        let dropped = max(0, messages.count - limit)
        return SubagentTranscript(
            messages: Array(messages.suffix(limit)), droppedRows: dropped, prompt: prompt
        )
    }

    /// The same reading, taken off Swarm's own stored rows rather than off the CLI's file.
    ///
    /// **Why there is a second source at all.** `output_file` is named on `task_notification`,
    /// which is the line that ENDS a subagent, so for the whole of the run there is no path to
    /// read and the pane had nothing to say. Swarm is not blind for that period: every line the
    /// subagent produces arrives on the parent's own stream carrying `parent_tool_use_id`, and
    /// those rows are already stored, already drawn nested behind a hairline in the transcript,
    /// and already keyed by the `tool_use_id` the subagent carries.
    ///
    /// It is `parse` and not a parser of its own, because a stream-json `assistant` or `user`
    /// line and a line of Claude Code's transcript file are the same object: a `type` and a
    /// `message` whose `content` is a string or the usual array of blocks. The extra keys a
    /// stream line carries (`parent_tool_use_id`, `session_id`) are ignored by the reader, as
    /// every key it does not know is.
    ///
    /// The file stays the honest source once there is one: it is what the CLI wrote for this
    /// task, and this is what Swarm happened to see while it was being written.
    ///
    /// - Parameter streamLines: the stored payloads of the nested rows, in the order they arrived.
    public static func live(streamLines: [Data], sessionID: SessionID) -> SubagentTranscript {
        parse(streamLines.map { String(decoding: $0, as: UTF8.self) }.joined(separator: "\n"), sessionID: sessionID)
    }

    /// What a background command printed, as the one block of text it is.
    ///
    /// No parsing, because there is nothing to parse: it is the bytes a program wrote to a
    /// terminal. Trimmed only, so an empty capture is an empty transcript and the pane can say
    /// "nothing yet" rather than draw a blank block.
    public static func command(_ text: String) -> SubagentTranscript {
        SubagentTranscript(printed: text.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    // MARK: - Identity

    /// The id a row that was never stored is drawn under.
    ///
    /// **Negative, and that is not decoration.** The window caches how a tool call is drawn under
    /// the row's id alone (`TranscriptPresentationCache`, whose whole argument is that a stored
    /// payload is written once so a presentation taken from it cannot go stale). A `messages`
    /// rowid is a positive `AUTOINCREMENT`, so a synthetic row numbered from zero would read the
    /// label of whichever real row shared its number, in whichever workspace was open.
    ///
    /// Derived from the payload rather than from the position, because this pane re-reads once a
    /// second and hands over from the live stream to the file the moment the subagent ends. Rows
    /// numbered by position would be renumbered by either of those, which moves a cached
    /// presentation onto the wrong call and closes whatever row the reader had opened.
    ///
    /// FNV-1a rather than `Hasher`, because `Hasher` is seeded per process and this wants the same
    /// answer for the same bytes every time, including across the two sources.
    public static func rowID(for payload: Data) -> Int64 {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in payload {
            hash = (hash ^ UInt64(byte)) &* 0x0000_0100_0000_01b3
        }
        return -Int64(hash & 0x7fff_ffff_ffff_ffff) - 1
    }

    /// The same id, stepped down until it is one nothing else in this transcript is using.
    ///
    /// Two identical payloads would otherwise be two rows with one identity, which a list draws
    /// in whatever order it pleases. Every real line carries a `uuid`, so this only ever fires on
    /// a source that has repeated itself.
    private static func identifier(for payload: Data, avoiding used: inout Set<Int64>) -> Int64 {
        var id = rowID(for: payload)
        while used.contains(id) { id -= 1 }
        used.insert(id)
        return id
    }
}

/// Reads one provider-owned NDJSON transcript in full, then only bytes appended after each read.
/// It keeps partial lines for the next read, restarts after truncation, and applies a byte limit.
public actor TranscriptLogReader {
    public enum Format: Sendable {
        case claude(sessionID: SessionID)
        case codex(sessionID: SessionID, providerSessionID: String)
    }

    public static let byteLimit = 64 * 1024 * 1024

    /// How many messages a chat keeps in memory.
    ///
    /// **The read is cheap and the fold is not.** Only appended bytes are parsed, so a long chat
    /// costs one parse per new line; but every caller rebuilds its whole row list from the
    /// messages each time the file grows, which is four times a second while a turn runs. Without
    /// a cap that is thousands of JSON payloads per second on a chat that has been going for a
    /// day. The older messages are counted in `droppedRows` and the chat says so.
    public static let messageLimit = SubagentTranscript.rowLimit

    private struct LineRecord: Sendable {
        var bytes: Int
        var messages: Int
    }

    private let url: URL
    private let format: Format
    private let limit: Int
    private var offset: UInt64 = 0
    private var pending = Data()
    private var discardsLine = false
    private var records: [LineRecord] = []
    private var recordStart = 0
    private var retainedBytes = 0
    private var messages: [Message] = []
    private var messageStart = 0
    private var droppedRows = 0
    private var used = Set<Int64>()
    private var nextSequence = 0
    /// What the read in progress has appended, and whether it also moved the front of the list.
    /// See `SubagentTranscript.appended`, which is where these two become one number.
    private var appendedThisRead = 0
    private var rebuiltThisRead = false

    public init(url: URL, format: Format, byteLimit: Int = TranscriptLogReader.byteLimit) {
        self.url = url
        self.format = format
        self.limit = max(1, byteLimit)
    }

    public func read() throws -> SubagentTranscript {
        _ = try consumeAppended()
        let visible = Array(messages.dropFirst(messageStart))
        return SubagentTranscript(
            messages: visible, droppedRows: droppedRows, appended: visible.count
        )
    }

    /// Nil when the file has not grown since the last read, which is most seconds of a pane
    /// watching a chat nobody is typing into.
    ///
    /// A caller turns these messages into rows, and for a chair log that is thousands of rows and
    /// a JSON payload read for each one, once a second, to find that nothing changed. The answer
    /// is the same file the pane already has, so it says so rather than building it again.
    public func readIfChanged() throws -> SubagentTranscript? {
        guard try consumeAppended() else { return nil }
        let visible = Array(messages.dropFirst(messageStart))
        return SubagentTranscript(
            messages: visible, droppedRows: droppedRows,
            appended: rebuiltThisRead ? visible.count : min(appendedThisRead, visible.count)
        )
    }

    /// Reads the bytes added since the last read. True when there were any, or when a truncated
    /// file sent the reader back to the beginning.
    private func consumeAppended() throws -> Bool {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let size = try handle.seekToEnd()
        var changed = false
        appendedThisRead = 0
        // A first read has nothing behind it to fold onto, and a truncated file invalidates
        // everything a caller drew from the bytes that are gone.
        rebuiltThisRead = offset == 0
        if size < offset {
            reset()
            rebuiltThisRead = true
            changed = true
        }
        if offset == 0, size > UInt64(limit) {
            offset = size - UInt64(limit)
            discardsLine = true
            droppedRows = 1
        }
        try handle.seek(toOffset: offset)
        while let chunk = try handle.read(upToCount: min(1024 * 1024, limit)), !chunk.isEmpty {
            offset += UInt64(chunk.count)
            consume(chunk)
            changed = true
        }
        return changed
    }

    private func reset() {
        offset = 0
        pending.removeAll(keepingCapacity: true)
        discardsLine = false
        records.removeAll(keepingCapacity: true)
        recordStart = 0
        retainedBytes = 0
        messages.removeAll(keepingCapacity: true)
        messageStart = 0
        droppedRows = 0
        used.removeAll(keepingCapacity: true)
        nextSequence = 0
    }

    private func consume(_ chunk: Data) {
        var input = chunk
        if discardsLine {
            guard let newline = input.firstIndex(of: 0x0a) else { return }
            input = Data(input[input.index(after: newline)...])
            discardsLine = false
        }
        pending.append(input)

        var lineStart = pending.startIndex
        while let newline = pending[lineStart...].firstIndex(of: 0x0a) {
            appendLine(Data(pending[lineStart..<newline]), bytes: newline - lineStart + 1)
            lineStart = pending.index(after: newline)
        }
        if lineStart != pending.startIndex {
            pending = Data(pending[lineStart...])
        }
        if pending.count > limit {
            pending.removeAll(keepingCapacity: true)
            discardsLine = true
            droppedRows = max(1, droppedRows)
        }
        trim()
    }

    private func appendLine(_ line: Data, bytes: Int) {
        let text = String(decoding: line, as: UTF8.self)
        let parsed: [Message]
        switch format {
        case .claude(let sessionID):
            parsed = SubagentTranscript.parseChair(text, sessionID: sessionID, limit: nil).messages
        case .codex(let sessionID, let providerSessionID):
            parsed = InteractiveChatTranscript.parseCodex(
                text, sessionID: sessionID, providerSessionID: providerSessionID, limit: nil
            ).messages
        }

        for var message in parsed {
            while used.contains(message.id) { message.id -= 1 }
            used.insert(message.id)
            message.seq = nextSequence
            nextSequence += 1
            messages.append(message)
            appendedThisRead += 1
        }
        records.append(LineRecord(bytes: bytes, messages: parsed.count))
        retainedBytes += bytes
    }

    private func trim() {
        var trimmed = false
        while messages.count - messageStart > Self.messageLimit {
            messageStart += 1
            droppedRows += 1
            trimmed = true
        }
        while retainedBytes + pending.count > limit, recordStart < records.count {
            let record = records[recordStart]
            recordStart += 1
            retainedBytes -= record.bytes
            messageStart += record.messages
            droppedRows += record.messages
            trimmed = true
        }
        if trimmed, droppedRows == 0 { droppedRows = 1 }
        if recordStart > 1024, recordStart * 2 > records.count {
            records.removeFirst(recordStart)
            recordStart = 0
        }
        if messageStart > 1024, messageStart * 2 > messages.count {
            messages.removeFirst(messageStart)
            messageStart = 0
        }
        if trimmed { used = Set(messages.dropFirst(messageStart).map(\.id)) }
        // The front moved, so every row a caller drew is now at a different place in the list and
        // the oldest of them are not in it at all. Nothing can be folded onto that.
        if trimmed { rebuiltThisRead = true }
    }
}

/// Reads of a chair's Claude Code transcript.
public enum ChairTranscriptOutput: Sendable {
    /// A Codex chair writes a Codex rollout, which the Claude reader turns into one opaque row
    /// per line (`session_meta`, `event_msg`, `response_item`), so the chair's provider decides.
    public static func reader(
        path: String?, sessionID: SessionID, provider: String? = nil, chairID: SwarmChairID? = nil
    ) -> TranscriptLogReader? {
        guard let path, !path.isEmpty else { return nil }
        let format: TranscriptLogReader.Format = if provider == AgentKind.codex.rawValue, let chairID {
            .codex(sessionID: sessionID, providerSessionID: chairID.rawValue)
        } else {
            .claude(sessionID: sessionID)
        }
        return TranscriptLogReader(url: URL(fileURLWithPath: path), format: format)
    }

    public static func read(
        _ reader: TranscriptLogReader?
    ) async -> Result<SubagentTranscript, SubagentOutput.Failure> {
        guard let reader else { return .failure(.noFile) }
        do {
            return .success(try await reader.read())
        } catch let error as CocoaError where error.code == .fileNoSuchFile {
            return .failure(.missing)
        } catch {
            return .failure(.unreadable(error.localizedDescription))
        }
    }

    /// A success holding nil means the log has not grown, so the pane keeps the rows it has.
    public static func readIfChanged(
        _ reader: TranscriptLogReader?
    ) async -> Result<SubagentTranscript?, SubagentOutput.Failure> {
        guard let reader else { return .failure(.noFile) }
        do {
            return .success(try await reader.readIfChanged())
        } catch let error as CocoaError where error.code == .fileNoSuchFile {
            return .failure(.missing)
        } catch {
            return .failure(.unreadable(error.localizedDescription))
        }
    }

    /// Reads only the bounded head needed to name a session from its first user prompt.
    public static func firstUserPrompt(path: String) -> String? {
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: url.path),
              let handle = try? FileHandle(forReadingFrom: url)
        else { return nil }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: SubagentOutput.tailBytes) else { return nil }
        let transcript = SubagentTranscript.parseChair(
            String(decoding: data, as: UTF8.self), sessionID: SessionID("chair")
        )
        guard let first = transcript.messages.first(where: { $0.kind == .user }) else {
            return nil
        }
        let text = UserTurnPrompt.text(in: first.payload)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }
}

/// Reading a subagent's transcript off disk.
///
/// A file Swarm does not own, in a directory Swarm does not own, written by another process that
/// may still be writing it. So every failure is a sentence rather than a throw: the pane's job is
/// to say what it can see, and "the CLI has not written this yet" is a thing it can see.
public enum SubagentOutput: Sendable {
    public enum Failure: Error, Sendable, Hashable {
        /// The notification never named a file.
        case noFile
        /// It named one that is not there. The CLI writes the path into the notification and the
        /// file a moment later, so this is briefly true for a subagent that has just ended.
        case missing
        /// It is there and could not be read: permissions, or a symlink whose target has gone.
        case unreadable(String)
    }

    /// How much of the end of the file is read.
    ///
    /// The pane re-reads once a second while the subagent works (`SubagentPane.refreshSeconds`),
    /// and that is only affordable against a bound. It is the END that is read, because that is
    /// where the answer is and because the pane only renders the last
    /// `SubagentTranscript.rowLimit` rows anyway. A quarter of a megabyte holds far more
    /// than that many lines of anything the capture contained, so in practice this reads whole
    /// files and exists for the one that is not.
    public static let tailBytes = 256 * 1024

    /// Read and parse one subagent's output.
    ///
    /// `path` is `task_notification.output_file`. For an agent it is a symlink to NDJSON in
    /// Claude Code's transcript shape; for a background command it is plain stdout, which is why
    /// the kind is asked for rather than sniffed. Parsing one as the other is what made a
    /// background command's pane empty.
    public static func read(path: String?, kind: SubagentKind = .agent, sessionID: SessionID)
        -> Result<SubagentTranscript, Failure> {
        guard let path, !path.isEmpty else { return .failure(.noFile) }
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: url.path) else { return .failure(.missing) }
        do {
            let text = try tail(of: url)
            return .success(kind.writesTranscript
                ? SubagentTranscript.parse(text, sessionID: sessionID)
                : SubagentTranscript.command(text))
        } catch {
            return .failure(.unreadable(error.localizedDescription))
        }
    }

    /// The last `tailBytes` of a file, starting at a line boundary.
    ///
    /// The first partial line is dropped rather than handed on: half a JSON object would be a
    /// skipped line in the NDJSON case and half a word of output in the other, and the second of
    /// those is the one somebody would have believed. A file smaller than the bound is returned
    /// whole, first line and all.
    static func tail(of url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let size = Int(try handle.seekToEnd())
        guard size > tailBytes else {
            try handle.seek(toOffset: 0)
            let data = try handle.readToEnd() ?? Data()
            return String(decoding: data, as: UTF8.self)
        }
        try handle.seek(toOffset: UInt64(size - tailBytes))
        let data = try handle.readToEnd() ?? Data()
        let text = String(decoding: data, as: UTF8.self)
        guard let newline = text.firstIndex(where: \.isNewline) else { return text }
        return String(text[text.index(after: newline)...])
    }
}

extension SubagentOutput.Failure {
    /// What the pane says instead of a transcript.
    ///
    /// Worded per kind, because "this subagent's output" said of a `git push` running in the
    /// background is the same category error that put the two in one list. The empty
    /// `output_file` is the ordinary case for a background command rather than a fault, so
    /// `.noFile` says so plainly instead of blaming the agent for not telling us.
    public func sentence(_ kind: SubagentKind = .agent) -> String {
        switch (self, kind) {
        case (.noFile, .agent):
            "The agent did not say where this subagent's output was written."
        case (.noFile, .command):
            "The agent did not capture this command's output, so there is nothing to show."
        case (.missing, .agent):
            "The agent has not written this subagent's output yet."
        case (.missing, .command):
            "This command has not printed anything yet."
        case (.unreadable(let reason), _):
            "This \(kind.noun)'s output could not be read. \(reason)"
        }
    }
}
