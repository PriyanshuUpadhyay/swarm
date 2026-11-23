// SwarmName: names a chat's branch from its first prompt with Apple's on-device model (ADR 0158).
// `swarm` runs it with the prompt's start as its one argument and a 3 s deadline. It prints one name, or
// exits 1 with no output; swarm then names the branch by its word rules.
import Foundation
import FoundationModels

@Generable
struct Name {
    @Guide(description: "2 to 5 lowercase English words joined by hyphens that name the coding task, such as fix-flaky-smoke-test")
    var branch: String
}

let arguments = CommandLine.arguments
guard arguments.count == 2, case .available = SystemLanguageModel.default.availability else { exit(1) }
let prompt = arguments[1]

let session = LanguageModelSession(instructions: """
    You name git branches. Read the developer's request to a coding agent and give a short \
    branch name for the task it asks for. Use the task's main verb and object. Leave out \
    politeness and filler words. Use only words that the request has, or their plain forms.
    """)
do {
    let reply = try await session.respond(to: prompt, generating: Name.self)
    print(reply.content.branch)
} catch {
    exit(1)
}
