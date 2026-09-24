import Foundation

public enum Composer {
    /// The text that leaves the box, or nil when nothing should be sent.
    public static func outgoing(_ draft: String) -> String? {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.isEmpty ? nil : text
    }

    /// Adds a path as its own token and leaves the caret ready for more text.
    public static func appending(path: String, to draft: String, prefix: String = "") -> String {
        let separator = draft.isEmpty || draft.last?.isWhitespace == true ? "" : " "
        return draft + separator + prefix + path + " "
    }

    public static func removing(path: String, prefix: String = "", from draft: String) -> String {
        let token = prefix + path
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
        wholePathRange(of: path, in: draft, from: draft.startIndex) != nil
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
            cursor = range.upperBound
        }
        return nil
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
}

public struct ComposerAttachment: Identifiable, Hashable, Sendable {
    public var path: String
    public var id: String { path }

    public init(path: String) {
        self.path = path
    }

    public var name: String { (path as NSString).lastPathComponent }
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
