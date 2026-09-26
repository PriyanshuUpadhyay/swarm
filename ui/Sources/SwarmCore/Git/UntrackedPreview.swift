import Foundation

enum UntrackedPreview {
    static func read(_ path: String, in root: String) throws -> String {
        let preview: WorkspaceFilePreview
        do { preview = try WorkspaceFiles.read(in: root, path: path) }
        catch let error as WorkspaceReadError { return error.description }
        switch preview {
        case .notice(let message): return message
        case .text(let text):
            // Git quotes unusual names using C string syntax. JSON's string escaping also
            // protects quotes, control characters, and backslashes in these patch headers.
            let encoder = JSONEncoder()
            encoder.outputFormatting = .withoutEscapingSlashes
            let oldName = String(decoding: try encoder.encode("a/" + path), as: UTF8.self)
            let newName = String(decoding: try encoder.encode("b/" + path), as: UTF8.self)
            var lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
            if text.hasSuffix("\n") || text.isEmpty { lines.removeLast() }
            let body = lines.map { "+" + $0 }.joined(separator: "\n")
            let ending = !text.isEmpty && !text.hasSuffix("\n") ? "\n\\ No newline at end of file\n" : "\n"
            return "diff --git \(oldName) \(newName)\nnew file mode 100644\n--- /dev/null\n+++ \(newName)\n@@ -0,0 +1,\(lines.count) @@\n" + body + ending
        }
    }
}
