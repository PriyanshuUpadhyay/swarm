// OwnerChoices and TabStrip come from production Swift sources at compile time.
// These boundary stubs supply types whose behaviour is outside this fixture.
import Foundation
public struct Prefs: Codable, Equatable, Sendable { public init() {} }
public struct ProjectDefaults: Codable, Equatable, Sendable {
    public var worktreeFolder: String?
    public var branchPrefix: String?
}
public enum WorkspaceNode {
    public static func removedPath(for path: String) -> String { path + "#removed" }
}
public struct ChatRow { public var session: String = "" }
public enum ChatTitle {
    public static func nonblank(_ text: String?) -> String? {
        let text = text?.trimmingCharacters(in: .whitespacesAndNewlines)
        return text?.isEmpty == false ? text : nil
    }
    public static func key(_ session: String) -> String { session }
}
public struct SessionRowPresentation {
    public enum State { case live }
    public var state: State = .live
    public static func make(_ row: ChatRow, now: Int) -> Self { Self() }
}
let data = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
let choices = try JSONDecoder().decode(OwnerChoices.self, from: data)
let expected = CommandLine.arguments[2]
guard choices.names[expected] == "From web", choices.pinned.contains(expected) else {
    fatalError("Web choices were not read by the Swift decoder")
}
print("PASS production OwnerChoices + TabStrip Swift decoders")
