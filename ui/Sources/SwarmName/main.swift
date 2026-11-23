// SwarmName: names a chat's branch from its first prompt with Apple's on-device model (ADR 0158).
// `swarm` runs it with the prompt's start as its one argument and a 3 s deadline. It prints one name, or
// exits 1 with no output; swarm then names the branch by its word rules.
import Foundation
import FoundationModels

@Generable
struct Name {
    @Guide(description: "The branch name: 2 to 4 lowercase words joined by hyphens, verb first")
    var branch: String
}

let arguments = CommandLine.arguments
guard arguments.count == 2, case .available = SystemLanguageModel.default.availability else { exit(1) }
let prompt = arguments[1]

let session = LanguageModelSession(instructions: """
    You name git branches. A developer wrote a request to a coding agent. Give a short branch \
    name for the work the request asks for.
    - Start with a verb in its plain form, such as fix, add, use, remove, rename, refactor, \
    update, investigate, review, document, explain.
    - When the request says "make X do Y", use the verb Y, not make.
    - When the request proposes an idea, such as "can we use X for Y", name the change it proposes.
    - Follow the verb with the thing the work changes, in 1 to 3 words.
    - Leave out version numbers, issue numbers and file extensions.
    - Keep the request's own names for products, files and features.
    - Leave out politeness and filler, such as please, can, we, maybe.
    - When the request is a question, use investigate or explain.
    - When the request asks for no change, such as a greeting or a thank you, use only its own words.
    Examples:
    "Can you fix the flaky smoke test please" -> fix-flaky-smoke-test
    "Bump serde to 1.0.220 and fix what breaks" -> update-serde
    "why does the build take so long on CI?" -> investigate-slow-ci-build
    "make the save dialog open in the last folder" -> open-save-dialog-last-folder
    "could we use redis to cache the search results?" -> cache-search-results-redis
    """)
do {
    // Greedy sampling, so one prompt always gives one name.
    let options = GenerationOptions(sampling: .greedy)
    let reply = try await session.respond(to: prompt, generating: Name.self, options: options)
    print(reply.content.branch)
} catch {
    exit(1)
}
