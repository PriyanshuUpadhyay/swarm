import Foundation

public enum Composer {
    /// The text that leaves the box, or nil when nothing should be sent.
    public static func outgoing(_ draft: String) -> String? {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    /// The text to type into a CLI, so the Enter after it can only submit. A typed ESC is the
    /// Escape key and interrupts the turn, so control characters other than LF and TAB go, and a
    /// CR is Enter, so CR and CRLF become LF. Claude sends nothing when it must remove invisible
    /// characters, so a Claude chat gets Claude's own cleaning, which does both of those too.
    /// A trailing `\` turns Enter into a newline, and a last word that starts with `@`, `$`, or
    /// `/` or ends with `:` leaves a popup open that takes Enter, so each gets one space.
    public static func submission(_ text: String, provider: String?) -> String {
        // A pane started with no provider name (`swarm spawn -- <path>/claude`) may be Claude.
        let cleaned = provider == "claude" || provider == nil
            ? ClaudeInvisibleText.cleaned(text) : withoutControlCharacters(text)
        guard cleaned.last?.isWhitespace == false,
              let word = cleaned.split(whereSeparator: \.isWhitespace).last, let first = word.first,
              word.hasSuffix("\\") || word.hasSuffix(":") || "@$/".contains(first)
        else { return cleaned }
        return cleaned + " "
    }

    private static func withoutControlCharacters(_ text: String) -> String {
        var scalars = String.UnicodeScalarView()
        var afterCR = false
        for scalar in text.unicodeScalars where !isControl(scalar) {
            if scalar == "\n", afterCR {
                afterCR = false
                continue
            }
            afterCR = scalar == "\r"
            scalars.append(afterCR ? "\n" : scalar)
        }
        return String(scalars)
    }

    /// C0 controls and DEL, but not TAB, LF, or CR, which becomes LF.
    private static func isControl(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.value {
        case 0x00...0x08, 0x0B, 0x0C, 0x0E...0x1F, 0x7F: true
        default: false
        }
    }

    /// Adds a path as its own token and leaves the caret ready for more text.
    public static func appending(path: String, to draft: String, prefix: String = "") -> String {
        let separator = draft.isEmpty || draft.last?.isWhitespace == true ? "" : " "
        return draft + separator + prefix + pathToken(path) + " "
    }

    public static func removing(path: String, prefix: String = "", from draft: String) -> String {
        let quoted = prefix + pathToken(path)
        let token = wholePathRange(of: quoted, in: draft, from: draft.startIndex) != nil
            ? quoted : prefix + path
        var result = ""
        var cursor = draft.startIndex
        while let range = wholePathRange(of: token, in: draft, from: cursor) {
            result += draft[cursor..<range.lowerBound]
            cursor = range.upperBound
            if cursor < draft.endIndex, draft[cursor].isWhitespace {
                cursor = draft.index(after: cursor)
            }
        }
        result += draft[cursor...]
        return result
    }

    public static func contains(path: String, in draft: String) -> Bool {
        wholePathRange(of: pathToken(path), in: draft, from: draft.startIndex) != nil
            || wholePathRange(of: path, in: draft, from: draft.startIndex) != nil
    }

    public static func retainedAttachments(
        _ attachments: [ComposerAttachment], in draft: String
    ) -> [ComposerAttachment] {
        attachments.filter { contains(path: $0.path, in: draft) }
    }

    private static func wholePathRange(
        of path: String, in draft: String, from start: String.Index
    ) -> Range<String.Index>? {
        guard !path.isEmpty else { return nil }
        var cursor = start
        while let range = draft.range(of: path, range: cursor..<draft.endIndex) {
            let leftBounded = range.lowerBound == draft.startIndex
                || draft[draft.index(before: range.lowerBound)].isWhitespace
            let rightBounded = range.upperBound == draft.endIndex
                || draft[range.upperBound].isWhitespace
            if leftBounded && rightBounded { return range }
            cursor = draft.index(after: range.lowerBound)
        }
        return nil
    }

    private static func pathToken(_ path: String) -> String {
        guard path.contains(where: \.isWhitespace) else { return path }
        return "\"" + path.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"") + "\""
    }
}

public struct ComposerAttachmentContext: Equatable, Sendable {
    public let sessionID: String
    public let generation: Int

    public init(sessionID: String, generation: Int) {
        self.sessionID = sessionID
        self.generation = generation
    }

    public func matches(sessionID: String, generation: Int) -> Bool {
        self.sessionID == sessionID && self.generation == generation
    }
}

/// Tracks submissions by chat, so a result can never clear another chat's draft.
public struct ComposerSendState: Sendable, Equatable {
    private var submittedDrafts: [String: String] = [:]

    public init() {}

    public func isSending(sessionID: String) -> Bool {
        submittedDrafts[sessionID] != nil
    }

    public mutating func begin(sessionID: String, draft: String) -> String? {
        guard submittedDrafts[sessionID] == nil,
              let message = Composer.outgoing(draft) else { return nil }
        submittedDrafts[sessionID] = draft
        return message
    }

    public mutating func finish(
        sessionID: String, currentDraft: String, succeeded: Bool
    ) -> String {
        guard let submitted = submittedDrafts.removeValue(forKey: sessionID) else {
            return currentDraft
        }
        guard succeeded, currentDraft == submitted else { return currentDraft }
        return ""
    }
}

/// Stores one draft per chat in the app preferences.
public struct ComposerDraftStore {
    private let defaults: UserDefaults
    private let keyPrefix: String

    public init(defaults: UserDefaults = .standard, keyPrefix: String = "composer.draft.") {
        self.defaults = defaults
        self.keyPrefix = keyPrefix
    }

    public func draft(for sessionID: String) -> String {
        defaults.string(forKey: keyPrefix + sessionID) ?? ""
    }

    public func save(_ draft: String, for sessionID: String) {
        let key = keyPrefix + sessionID
        if draft.isEmpty {
            defaults.removeObject(forKey: key)
        } else {
            defaults.set(draft, forKey: key)
        }
    }

    public func prune(keeping sessionIDs: Set<String>) {
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(keyPrefix) {
            let sessionID = String(key.dropFirst(keyPrefix.count))
            if !sessionIDs.contains(sessionID) { defaults.removeObject(forKey: key) }
        }
    }
}

public struct ComposerAttachment: Identifiable, Hashable, Sendable {
    public var path: String
    public var id: String { path }

    public init(path: String) {
        self.path = path
    }

    public var name: String { (path as NSString).lastPathComponent }
}

public enum ComposerAttachmentError: LocalizedError {
    case notAFile

    public var errorDescription: String? { "Only files can be attached." }
}

public enum ComposerAttachmentStore {
    public static func saveImage(
        _ data: Data, fileExtension: String, scratchDirectory: String
    ) throws -> ComposerAttachment {
        let directory = (scratchDirectory as NSString).appendingPathComponent("composer-images")
        try FileManager.default.createDirectory(
            atPath: directory, withIntermediateDirectories: true
        )
        let safeExtension = fileExtension.lowercased().filter { $0.isLetter || $0.isNumber }
        let name = UUID().uuidString + "." + (safeExtension.isEmpty ? "png" : safeExtension)
        let path = (directory as NSString).appendingPathComponent(name)
        try data.write(to: URL(fileURLWithPath: path), options: .atomic)
        return ComposerAttachment(path: path)
    }

    public static func importFile(
        at source: String, scratchDirectory: String
    ) throws -> ComposerAttachment {
        let sourceURL = URL(fileURLWithPath: source)
        let values = try sourceURL.resourceValues(forKeys: [.isRegularFileKey])
        guard values.isRegularFile == true else { throw ComposerAttachmentError.notAFile }
        guard FileManager.default.isReadableFile(atPath: sourceURL.path) else {
            throw CocoaError(.fileReadNoPermission)
        }
        guard isImage(pathExtension: sourceURL.pathExtension) else {
            return ComposerAttachment(path: sourceURL.path)
        }
        let data = try Data(contentsOf: sourceURL)
        return try saveImage(
            data, fileExtension: sourceURL.pathExtension, scratchDirectory: scratchDirectory
        )
    }

    public static func isImage(pathExtension: String) -> Bool {
        ["png", "jpg", "jpeg", "gif", "webp", "heic", "tif", "tiff", "bmp"]
            .contains(pathExtension.lowercased())
    }
}
