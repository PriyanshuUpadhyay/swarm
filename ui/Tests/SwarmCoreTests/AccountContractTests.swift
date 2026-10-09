import Foundation
import Testing
@testable import SwarmCore

@Suite("Native account contract")
struct AccountContractTests {
    @Test("reads the accepted native account fixture")
    func nativeAccount() throws {
        let list = try decode(SwarmAccountList.self, fixture: "work")
        #expect(list.auto == "work")
        #expect(list.accounts.first?.remainingPct == 70)
    }

    @Test("each auth and source state preserves uncertainty")
    func states() throws {
        #expect(try decode(SwarmAccountList.self, fixture: "spare-signed_out").accounts[0].authState == .signedOut)
        #expect(try decode(SwarmAccountList.self, fixture: "spare-unavailable").accounts[0].authState == .unavailable)
        #expect(try decode(SwarmAccountList.self, fixture: "spare-future").accounts[0].authState == .unavailable)
        #expect(try decode(SwarmAccountList.self, fixture: "work-unavailable").state == .unavailable)
        #expect(try decode(SwarmAccountList.self, fixture: "work-future").state == .unavailable)
        #expect(try decode(SwarmAccountList.self, fixture: "spare").state == .noSource)
    }

    @Test("each usage state and returned unit decodes", arguments: ["fresh", "stale", "missing", "failed", "no_source", "future"])
    func usageState(_ state: String) throws {
        let meter = try decode(SwarmUsage.self, fixture: "work-usage-" + state).meters[0]
        #expect(meter.state == (SwarmUsageState(rawValue: state) ?? .unavailable))
        #expect(meter.windowMinutes == 300)
        #expect(meter.resetTimeSeconds == 1_791_540_000)
        #expect(meter.source == "codex_app_server")
    }

    @Test("null and absent optional fields mean the same thing")
    func optionalFields() throws {
        let minimal = #"{"provider":"codex","label":"work","state":"missing"}"#
        let nulls = #"{"provider":"codex","label":"work","state":"missing","account":null,"window":null,"window_minutes":null,"used_pct":null,"reset_time_seconds":null,"source":null,"reason":null,"as_of_seconds":null}"#
        #expect(try wire(SwarmUsageMeter.self, minimal) == wire(SwarmUsageMeter.self, nulls))
        let account = #"{"name":"work","home":"/tmp/work","env":{},"auth_state":"unavailable","usage_state":"missing"}"#
        let accountNulls = #"{"name":"work","home":"/tmp/work","env":{},"auth_state":"unavailable","usage_state":"missing","email":null,"remaining_pct":null,"summary":null,"usage_source":null}"#
        #expect(try wire(SwarmAccount.self, account) == wire(SwarmAccount.self, accountNulls))
    }

    @Test("percentages accept endpoints and fractions and reject invalid values")
    func percentages() throws {
        for value in ["0", "100", "25.5"] {
            let meter = try wire(SwarmUsageMeter.self, "{\"provider\":\"codex\",\"label\":\"work\",\"state\":\"fresh\",\"used_pct\":\(value)}")
            #expect(meter.usedPct == Double(value))
        }
        for value in ["-1", "101", "1e999", "\"NaN\""] {
            #expect(throws: (any Error).self) {
                try wire(SwarmUsageMeter.self, "{\"provider\":\"codex\",\"label\":\"work\",\"state\":\"fresh\",\"used_pct\":\(value)}")
            }
            #expect(throws: (any Error).self) {
                try wire(SwarmAccount.self, "{\"name\":\"work\",\"home\":\"/tmp/work\",\"env\":{},\"remaining_pct\":\(value)}")
            }
        }
    }

    @Test("launch and picker preserve the full Claude environment")
    func launchAgreement() throws {
        let list = try decode(SwarmAccountList.self, fixture: "personal")
        let launch = try #require(SwarmLaunchAccount.resolve(.auto, from: list))
        let decision = SwarmAccountLoadDecision.loaded(list)
        #expect(decision.selection == .auto)
        #expect(SwarmAccountOption.account(for: decision.selection, in: decision.options) == launch)
        #expect(launch.environment == list.accounts[0].env)
        #expect(SwarmAccountOption.launchSelection(for: .auto, in: decision.options) == .named("personal"))
        #expect(list.autoEnvironment == launch.environment)
        #expect(launch.merging(into: ["PATH": "/usr/bin", "CLAUDE_CONFIG_DIR": "/other"])["CLAUDE_CONFIG_DIR"] == list.accounts[0].home)
        var injected = launch.environment
        injected["OWNER_WORK"] = "unsafe"
        #expect(SwarmLaunchAccount(name: "personal", provider: "claude", environment: injected) == nil)
        injected = launch.environment
        injected["AGENT_PROFILE_LABEL"] = "personal\nINJECT=1"
        #expect(SwarmLaunchAccount(name: "personal", provider: "claude", environment: injected) == nil)
    }

    @Test("Auto never falls back to another row and picker keeps returned order")
    func returnedAuto() throws {
        var list = try decode(SwarmAccountList.self, fixture: "work")
        var personal = list.accounts[0]
        personal.name = "personal"
        personal.remainingPct = 100
        list.accounts.append(personal)
        let options = SwarmAccountOption.choices(from: list)
        #expect(options.map(\.selection) == [.auto, .named("work"), .named("personal")])
        #expect(SwarmLaunchAccount.resolve(.auto, from: list)?.name == "work")
        list.auto = nil
        #expect(list.autoEnvironment.isEmpty)
        list.accounts[0].authState = .unavailable
        #expect(SwarmAccountOption.choices(from: list)[0].disabledReason == "Status unavailable")
        list.auto = "work"
        #expect(SwarmLaunchAccount.resolve(.auto, from: list) == nil)
    }

    @Test("typed mutation calls pass fixed argv and decode their replies")
    func actions() async throws {
        let login = try fixtureText("work-login")
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, args, _ in
            #expect(args == ["accounts", "login", "--provider", "codex", "--name", "work", "--revision", "opaque", "--json"])
            return ShellResult(status: 0, stdout: login, stderr: "")
        }
        let result = try await source.openLogin(.init(provider: "codex", name: "work", revision: "opaque"))
        #expect(result.state == .opened)
        #expect(result.pane == "pane-work")
        let usage = try fixtureText("work-usage")
        let refresh = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, args, _ in
            #expect(args == ["usage", "--refresh", "--provider", "codex", "--json"])
            return ShellResult(status: 0, stdout: usage, stderr: "")
        }
        #expect(try await refresh.refreshUsage(provider: "codex").meters[0].usedPct == 30)
        let reset = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, args, _ in
            #expect(args == ["accounts", "reset", "--revision", "opaque", "--json"])
            return ShellResult(status: 0, stdout: #"{"revision":"bundled"}"#, stderr: "")
        }
        #expect(try await reset.resetAccounts(revision: "opaque").revision == "bundled")
    }

    @Test("invalid new names never invoke the CLI", arguments: ["", "auto", "default", "../work", "two words", "WORK", "work;touch", "work\n", "é", ".."])
    func invalidName(_ name: String) async {
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, _, _ in
            Issue.record("Invalid input reached the CLI")
            return ShellResult(status: 1, stdout: "", stderr: "")
        }
        await #expect(throws: SwarmProfileError.self) {
            try await source.openLogin(.init(provider: "codex", name: name, revision: "opaque"))
        }
    }

    @Test("login passes the pane directory as one fixed argv value")
    func loginDirectory() async throws {
        let login = try fixtureText("work-login")
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp/scratch") { _, args, _ in
            #expect(args == ["accounts", "login", "--provider", "codex", "--name", "work",
                             "--revision", "opaque", "--cwd", "/tmp/work project", "--json"])
            return ShellResult(status: 0, stdout: login, stderr: "")
        }
        let request = SwarmAccountLoginRequest(provider: "codex", name: "work", revision: "opaque", directory: "/tmp/work project")
        #expect(try await source.openLogin(request).pane == "pane-work")
    }

    private func wire<T: Decodable>(_ type: T.Type, _ text: String) throws -> T {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(type, from: Data(text.utf8))
    }

    private func fixtureText(_ name: String) throws -> String {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("Fixtures/accounts/\(name).json")
        return try String(contentsOf: url, encoding: .utf8)
    }

    private func decode<T: Decodable>(_ type: T.Type, fixture: String) throws -> T {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("Fixtures/accounts/\(fixture).json")
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(type, from: Data(contentsOf: url))
    }
}
