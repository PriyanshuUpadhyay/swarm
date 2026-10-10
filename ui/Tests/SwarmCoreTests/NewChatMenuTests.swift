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
        #expect(rows.map(\.id) == rows.map { .profile($0.name) })
        #expect(NewChatMenu.rows(profiles: [codeProfile, chatProfile], defaultProfile: "code.complex")
            .map(\.name) == ["code.complex", "chat"])
        #expect(NewChatMenu.rows(profiles: [codeProfile, emptyProfile]).map(\.name)
            == ["code.complex", "empty"])
        #expect(NewChatMenu.rows(profiles: []).isEmpty)
    }

    @Test("A failed read retains the last good profiles and appends a disabled error row")
    func failedRead() {
        let profiles = [SwarmProfile(name: "chat", runners: [])]
        var menu = NewChatMenu.State()
        menu.apply(profiles: profiles)
        let goodRows = menu.rows
        menu.record(error: "Read timed out")
        #expect(menu.profiles == profiles)
        #expect(Array(menu.rows.dropLast()) == goodRows)
        #expect(menu.rows.last?.name == "Could not read profiles. Read timed out")
        #expect(menu.rows.last?.isEnabled == false)
        #expect(menu.rows.last?.id == .readError)
        #expect(NewChatMenu.rows(profiles: [], error: "No file").map(\.name) == ["Could not read profiles. No file"])
        menu.apply(profiles: [SwarmProfile(name: "code", runners: [])])
        #expect(menu.error == nil)
        #expect(menu.rows.map(\.name) == ["code"])
        #expect(menu.rows.allSatisfy { $0.isEnabled })
    }

    @Test("A running read blocks another read and the interval starts at completion")
    func readInterval() {
        var menu = NewChatMenu.State()
        let startedAt = ContinuousClock().now
        let finishedAt = startedAt.advanced(by: .seconds(20))
        #expect(menu.reserveRead(at: startedAt) == true)
        #expect(menu.reserveRead(at: finishedAt) == false)
        menu.finishRead(at: finishedAt)
        #expect(menu.reserveRead(at: finishedAt.advanced(by: .milliseconds(4_999))) == false)
        #expect(menu.reserveRead(at: finishedAt.advanced(by: .seconds(5))) == true)
    }

    @Test("A cancelled read sets no stamp and can be retried at once")
    func cancelledRead() {
        var menu = NewChatMenu.State()
        let now = ContinuousClock().now
        #expect(menu.reserveRead(at: now) == true)
        menu.finishRead(at: nil)
        #expect(menu.reserveRead(at: now) == true)
        menu.finishRead(at: now)
        let nextReadAt = now.advanced(by: .seconds(5))
        #expect(menu.reserveRead(at: nextReadAt) == true)
        menu.finishRead(at: nil)
        #expect(menu.reserveRead(at: nextReadAt) == true)
    }

    @Test("A failed read observes the same completion interval")
    func failedReadInterval() {
        var menu = NewChatMenu.State()
        let now = ContinuousClock().now
        #expect(menu.reserveRead(at: now) == true)
        menu.record(error: "Read timed out")
        menu.finishRead(at: now)
        #expect(menu.reserveRead(at: now) == false)
        #expect(menu.reserveRead(at: now.advanced(by: .seconds(5))) == true)
    }
}
