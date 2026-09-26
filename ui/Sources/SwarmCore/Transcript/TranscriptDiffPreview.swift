import Foundation
import TranscriptTool

/// Defaults to a limited display copy. Explicit full views and copy actions use all saved lines.
public struct TranscriptDiffPreview {
    public let patch: String
    public let omittedLines: Int
    public let shortenedLines: Int

    public init(_ diff: TranscriptDiff, full: Bool = false) {
        let lineLimit = full ? Int.max : 200
        let characterLimit = full ? Int.max : 400
        let encoder = JSONEncoder()
        encoder.outputFormatting = .withoutEscapingSlashes
        func quoted(_ prefix: String) -> String {
            let data = try! encoder.encode(prefix + diff.path)
            return String(decoding: data, as: UTF8.self)
        }
        let old = quoted("a/")
        let new = quoted("b/")
        var output = "diff --git \(old) \(new)\n--- \(old)\n+++ \(new)\n"
        var remaining = lineLimit
        var omitted = 0
        var shortened = 0
        for hunk in diff.hunks {
            let visible = hunk.lines.prefix(remaining)
            omitted += hunk.lines.count - visible.count
            remaining -= visible.count
            guard !visible.isEmpty else { continue }
            // A partial preview needs ranges for its visible lines, not the full hunk.
            let oldCount = visible.filter { $0.hasPrefix(" ") || $0.hasPrefix("-") }.count
            let newCount = visible.filter { $0.hasPrefix(" ") || $0.hasPrefix("+") }.count
            output += "@@ -\(hunk.oldStart),\(oldCount) +\(hunk.newStart),\(newCount) @@\n"
            for line in visible {
                let prefix = line.prefix(characterLimit)
                let clipped = prefix.endIndex != line.endIndex
                if clipped { shortened += 1 }
                output += prefix + (clipped ? "…" : "") + "\n"
            }
        }
        patch = output
        omittedLines = omitted
        shortenedLines = shortened
    }

    public var notice: String? {
        let parts = [
            omittedLines > 0 ? "\(omittedLines) patch lines omitted" : nil,
            shortenedLines > 0 ? "\(shortenedLines) long lines shortened" : nil,
        ].compactMap { $0 }
        return parts.isEmpty ? nil : "Preview only: " + parts.joined(separator: "; ") + "."
    }
}
