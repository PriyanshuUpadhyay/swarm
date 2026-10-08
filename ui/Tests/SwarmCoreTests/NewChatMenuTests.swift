import Testing
@testable import SwarmCore

@Suite("New chat menu")
struct NewChatMenuTests {
    @Test("The default comes first and captions use only the first runner")
    func profileRows() {
        let codeProfile = SwarmProfile(name: "code.complex", runners: [
            SwarmRunner(provider: "codex", model: "gpt-code", effort: "high"),
            SwarmRunner(provider: "claude", model: "opus", effort: "high"),
        ])
        let chatProfile = SwarmProfile(name: "chat", runners: [
            SwarmRunner(provider: "claude", model: "sonnet", effort: "low"),
        ])
        let emptyProfile = SwarmProfile(name: "empty", runners: [])
        let rows = NewChatMenu.rows(profiles: [codeProfile, chatProfile, emptyProfile])
        #expect(rows.map(\.name) == ["chat", "code.complex", "empty"])
        #expect(rows.map(\.caption) == ["claude · sonnet", "codex · gpt-code", nil])
        #expect(rows.map(\.isDefault) == [true, false, false])
        #expect(rows.map(\.id) == rows.map(\.name))
        #expect(NewChatMenu.rows(profiles: [codeProfile, chatProfile], defaultProfile: "code.complex")
            .map(\.name) == ["code.complex", "chat"])
        #expect(NewChatMenu.rows(profiles: [codeProfile, emptyProfile]).map(\.name)
            == ["code.complex", "empty"])
        #expect(NewChatMenu.rows(profiles: []).isEmpty)
    }
}
