import Foundation
import Testing
@testable import SwarmCore

@Suite("Row field choices")
struct RowFieldsTests {
    @Test("Agent launch and usage keys decode, including a zero cost")
    func agentKeys() throws {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let json = #"{"id":"chair","role":"chat","profile":"chat","runner":"chat#1","model":"gpt-6","effort":"high","account":"work","costUsd":0,"tokens":12345}"#
        let agent = try decoder.decode(SwarmAgent.self, from: Data(json.utf8))
        #expect(agent.profile == "chat")
        #expect(agent.runner == "chat#1")
        #expect(agent.model == "gpt-6")
        #expect(agent.effort == "high")
        #expect(agent.account == "work")
        #expect(agent.costUsd == 0)
        #expect(agent.tokens == 12345)
        let old = try decoder.decode(SwarmAgent.self, from: Data(#"{"id":"chair","role":"chat"}"#.utf8))
        #expect(old.profile == nil && old.runner == nil && old.model == nil)
        #expect(old.effort == nil && old.account == nil && old.costUsd == nil && old.tokens == nil)
        let null = try decoder.decode(SwarmAgent.self, from: Data(#"{"id":"chair","role":"chat","tokens":null,"costUsd":null}"#.utf8))
        #expect(null.tokens == nil && null.costUsd == nil)
    }

    @Test("Missing lists use the bundled defaults and an empty list stays empty")
    func defaults() throws {
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: Data("{}".utf8))
        #expect(choices.fields == RowFieldLists())
        #expect(choices.fields.project == [.title, .status])
        #expect(choices.fields.workspace == [.status, .title, .branch, .children, .age, .steps])
        #expect(choices.fields.chat == [.status, .title, .age, .steps, .children, .unread, .tokens])
        #expect(choices.fields.tab == [.status, .title, .provider, .unread, .tokens])
        let empty = try JSONDecoder().decode(OwnerChoices.self, from: Data(#"{"fields":{"chat":[]}}"#.utf8))
        #expect(empty.fields.chat.isEmpty)
        #expect(empty.fields.workspace == choices.fields.workspace)
    }

    @Test("Unknown names are skipped without losing known fields or their order")
    func unknownNames() throws {
        let json = #"{"fields":{"chat":["cost","future","model","title","tokens"],"tab":["unknown"]}}"#
        let choices = try JSONDecoder().decode(OwnerChoices.self, from: Data(json.utf8))
        #expect(choices.fields.chat == [.cost, .model, .title, .tokens])
        #expect(choices.fields.tab.isEmpty)
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: JSONEncoder().encode(choices)) == choices)
    }
}
