import Foundation

/// Reads prompt and CLI name records without loading the whole log into memory.
public enum ChairLogTitle {
    // Claude Code 2.1.292 embedded CLI source: ZW/oie at bytes 195642693/195643051
    // append custom-title/customTitle and ai-title/aiTitle; dEn/w4o at 191011383
    // read <log parent>/<sessionId>/custom-title.json. PK at 183292080 prefers customTitle.
    public static func claudeName(path: String) -> String? {
        var reader = ClaudeNameReader()
        return reader.name(path: path)
    }

    // codex-rs/rollout/src/session_index.rs: SessionIndexEntry, append_thread_name,
    // find_thread_names_by_ids. https://github.com/openai/codex/blob/main/codex-rs/rollout/src/session_index.rs
    public static func codexNames(home: URL) -> [String: String] {
        var names: [String: String] = [:]
        records(path: home.appendingPathComponent("session_index.jsonl").path) { object in
            guard let id = object["id"] as? String, let name = ChatTitle.nonblank(object["thread_name"] as? String) else { return }
            names[id] = name
        }
        return names
    }

    static func codexID(path: String) -> String? {
        guard let file = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return nil }
        defer { try? file.close() }
        guard let data = try? file.read(upToCount: 65_536), let line = data.split(separator: 10).first,
              let object = try? JSONSerialization.jsonObject(with: Data(line)) as? [String: Any],
              object["type"] as? String == "session_meta" else { return nil }
        return (object["payload"] as? [String: Any])?["id"] as? String
    }

    /// Read complete records with bounded memory; a large or partial record cannot supply a name.
    private static func records(path: String, consume: ([String: Any]) -> Void) {
        var reader = CLITitleRecordReader()
        reader.read(path: path, consume: consume)
    }

    public static func firstUserPrompt(path: String) -> String? {
        guard let file = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return nil }
        defer { try? file.close() }
        guard let data = try? file.read(upToCount: 256 * 1024) else { return nil }
        for line in data.split(separator: 0x0a) {
            guard let object = (try? JSONSerialization.jsonObject(with: Data(line))) as? [String: Any]
            else { continue }
            let content: Any?
            switch object["type"] as? String {
            case "user":
                content = (object["message"] as? [String: Any])?["content"]
            case "response_item":
                let payload = object["payload"] as? [String: Any]
                content = payload?["role"] as? String == "user" ? payload?["content"] : nil
            default:
                content = nil
            }
            let text: String?
            if let string = content as? String {
                text = string
            } else if let blocks = content as? [[String: Any]] {
                text = blocks.compactMap { $0["text"] as? String }.joined(separator: "\n")
            } else {
                text = nil
            }
            // `/clear` opens its log with a meta caveat and the `/clear` record, neither a prompt.
            guard var text, object["isMeta"] as? Bool != true,
                  !text.contains(ConversationBoundary.clearCommand) else { continue }
            text = text.replacingOccurrences(
                of: #"(?s)<command-name>.*?</command-name>"#,
                with: "", options: .regularExpression
            )
            text = text.replacingOccurrences(
                of: #"<[^>]+>"#, with: "", options: .regularExpression
            ).trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty, !text.hasPrefix("# AGENTS.md") else { continue }
            if let firstLine = text.components(separatedBy: .newlines).first(where: {
                !$0.trimmingCharacters(in: .whitespaces).isEmpty
            }) {
                return firstLine.trimmingCharacters(in: .whitespaces)
            }
        }
        return nil
    }
}

/// Keeps title state and the cut last record across a live Claude log's appends.
struct ClaudeNameReader {
    private static let titleMarkers = [Data("custom-title".utf8), Data("ai-title".utf8)]
    private var reader = CLITitleRecordReader()
    private var logStamp: CLINameFileStamp?
    private var sidecarStamp: CLINameFileStamp?
    private var custom: String?
    private var generated: String?
    private var hasCustomRecord = false
    private var sidecarTitle: String?
    var offset: UInt64 { reader.offset }
    var bytesRead: UInt64 { reader.bytesRead }

    mutating func name(path: String) -> String? {
        let log = URL(fileURLWithPath: path)
        let id = log.deletingPathExtension().lastPathComponent
        let stamp = CLINameFileStamp(path: path)
        if stamp != logStamp {
            if stamp == nil || logStamp == nil || stamp!.fileNumber != logStamp!.fileNumber
                || stamp!.size < reader.offset || (stamp!.size == logStamp!.size && stamp!.modified != logStamp!.modified) {
                reader.reset()
                custom = nil
                generated = nil
                hasCustomRecord = false
            }
            // Use a local reader so its callback can update the title fields without overlapping access.
            var scanning = reader
            scanning.read(path: path, markers: Self.titleMarkers) { object in
                guard object["sessionId"] as? String == id else { return }
                switch object["type"] as? String {
                case "custom-title":
                    guard let title = object["customTitle"] as? String else { return }
                    hasCustomRecord = true
                    custom = ChatTitle.nonblank(title)
                case "ai-title":
                    guard let title = object["aiTitle"] as? String else { return }
                    generated = ChatTitle.nonblank(title)
                default: break
                }
            }
            reader = scanning
            logStamp = stamp
        }
        let sidecar = log.deletingLastPathComponent().appendingPathComponent(id)
            .appendingPathComponent("custom-title.json")
        let savedStamp = CLINameFileStamp(path: sidecar.path)
        if savedStamp != sidecarStamp {
            sidecarTitle = nil
            if let data = try? Data(contentsOf: sidecar),
               let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                sidecarTitle = ChatTitle.nonblank(object["customTitle"] as? String)
            }
            sidecarStamp = savedStamp
        }
        return (hasCustomRecord ? custom : sidecarTitle) ?? generated
    }
}

private struct CLITitleRecordReader {
    private(set) var offset: UInt64 = 0
    private(set) var bytesRead: UInt64 = 0
    private var pending = Data()
    private var oversized = false

    mutating func reset() {
        offset = 0
        pending.removeAll(keepingCapacity: true)
        oversized = false
    }

    mutating func read(path: String, markers: [Data] = [], consume: ([String: Any]) -> Void) {
        guard let file = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return }
        defer { try? file.close() }
        do { try file.seek(toOffset: offset) } catch { return }
        while let chunk = try? file.read(upToCount: 65_536), !chunk.isEmpty {
            offset += UInt64(chunk.count)
            bytesRead += UInt64(chunk.count)
            var start = chunk.startIndex
            while start < chunk.endIndex {
                let newline = chunk[start...].firstIndex(of: 10)
                let end = newline ?? chunk.endIndex
                if !oversized {
                    if pending.count + end - start <= 256 * 1024 {
                        pending.append(contentsOf: chunk[start..<end])
                    } else {
                        pending.removeAll(keepingCapacity: true)
                        oversized = true
                    }
                }
                guard let newline else { break }
                if !oversized, markers.isEmpty || markers.contains(where: { pending.range(of: $0) != nil }),
                   let object = try? JSONSerialization.jsonObject(with: pending) as? [String: Any] {
                    consume(object)
                }
                pending.removeAll(keepingCapacity: true)
                oversized = false
                start = chunk.index(after: newline)
            }
        }
    }
}
