import Testing
@testable import SwarmWeb

@Suite("Shell script message")
struct ShellMessageTests {
    @Test("accepts only the folder picker shape")
    func shape() {
        #expect(ShellMessage(body: ["kind": "pickFolder"]) == .pickFolder)
        let refused: [Any] = [
            "pickFolder",
            ["kind": "pickfolder"],
            ["kind": 1],
            ["kind": "pickFolder", "path": "/etc"],
            [String: Any](),
            ["pickFolder"],
        ]
        for body in refused {
            #expect(ShellMessage(body: body) == nil, "\(body)")
        }
    }
}
