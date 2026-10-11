import Foundation
import Testing
@testable import SwarmCore

@Suite("Accounts page state")
struct AccountsPageModelTests {
    @Test("sections appear as each read finishes and failures stay local")
    func partialReads() throws {
        var model = AccountsPageModel()
        #expect(model.section("claude").message == "Reading accounts…")
        let generation = try start(&model)
        model.receiveAccounts(list(), provider: "codex", generation: generation)
        #expect(model.section("codex").accounts.map(\.name) == ["work"])
        #expect(model.section("claude").message == "Reading accounts…")
        model.failAccounts(provider: "claude", error: SwarmProfileError.failed("Status read failed"), generation: generation)
        #expect(model.section("claude").message == "Accounts unavailable")
        #expect(model.section("claude").error == "Status read failed")
        #expect(model.section("codex").accounts.count == 1)
        model.finishLoad(generation)
        #expect(!model.isLoading)
        #expect(model.errorMessage?.contains("Status read failed") == true)
    }

    @Test("empty, no-source and missing CLI have distinct text and actions")
    func sourceStates() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        model.receiveAccounts(list(accounts: []), provider: "codex", generation: generation)
        #expect(model.section("codex").message == "No accounts yet.")
        #expect(model.section("codex").canAdd)
        model.receiveAccounts(SwarmAccountList(provider: "agy", source: nil, accounts: [], auto: nil,
                                              state: .noSource, revision: "opaque"), provider: "agy", generation: generation)
        #expect(model.section("agy").message == "No account source in Swarm.")
        #expect(!model.section("agy").canAdd)
        #expect(model.section("agy").usageMessage == "No usage source in Swarm. View quota with /usage in the CLI.")
        model.failAccounts(provider: "claude", error: AccountsPageError.cliMissing("claude"), generation: generation)
        #expect(model.section("claude").message == "Install the Claude CLI to add an account.")
        #expect(!model.section("claude").canAdd)
    }

    @Test("wire unavailable state is not an empty ready list")
    func unavailableSource() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        var unavailable = list()
        unavailable.state = .unavailable
        model.receiveAccounts(unavailable, provider: "codex", generation: generation)
        #expect(model.section("codex").message == "Accounts unavailable")
        #expect(!model.section("codex").canAdd)
        #expect(model.errorMessage != nil)
    }

    @Test("repeated refreshes coalesce and obsolete results cannot overwrite state")
    func generations() throws {
        var model = AccountsPageModel()
        let old = try start(&model)
        let repeatRead = model.beginLoad()
        #expect(repeatRead == nil)
        model.invalidateReads()
        let current = try start(&model)
        var changed = list()
        changed.auto = nil
        changed.revision = "new"
        model.receiveAccounts(changed, provider: "codex", generation: current)
        model.receiveAccounts(list(), provider: "codex", generation: old)
        model.failAccounts(provider: "codex", error: SwarmProfileError.failed("Obsolete"), generation: old)
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 99)]), generation: old)
        model.finishLoad(old)
        #expect(model.revision == "new")
        #expect(model.section("codex").auto == nil)
        #expect(model.errorMessage == nil)
        #expect(model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0].remainingPct == nil)
        #expect(model.isLoading)
        model.finishLoad(current)
        #expect(!model.isLoading)
    }

    @Test("fresh endpoints and every returned window keep their actual units")
    func quotaWindows() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        var secondary = meter(usedPct: 100)
        secondary.window = "secondary"
        secondary.windowMinutes = 10_080
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 0), secondary]), generation: generation)
        let rows = model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)
        #expect(rows.map(\.remainingPct) == [100, 0])
        #expect(rows.map(\.title) == ["primary · 300 min", "secondary · 10080 min"])
        #expect(rows.allSatisfy { !$0.isOld })
        #expect(rows[0].source == "codex_app_server")
        #expect(rows[0].sampleTimeSeconds == 1_000)
        #expect(rows[0].resetTimeSeconds == 1_500)
    }

    @Test("missing and unsupported quota never invent a percentage")
    func absentQuota() throws {
        var model = AccountsPageModel()
        #expect(model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0].status == "No usage reading")
        let generation = try start(&model)
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: nil, state: .missing)]), generation: generation)
        #expect(model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0].remainingPct == nil)
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: nil, state: .noSource)]), generation: generation)
        #expect(model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0].status == "No usage source in Swarm")
    }

    @Test("failed quota keeps the previous sample, its source and its age")
    func failedQuota() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 30)]), generation: generation)
        var failed = meter(usedPct: nil, state: .failed)
        failed.reason = "Timed out"
        failed.asOfSeconds = nil
        model.receiveUsage(SwarmUsage(meters: [failed]), generation: generation, provider: "codex")
        let row = model.usageRows(provider: "codex", account: "work", nowSeconds: 1_010)[0]
        #expect(row.remainingPct == 70)
        #expect(row.isOld)
        #expect(row.status == "Usage unavailable")
        #expect(row.reason == "Timed out")
        #expect(row.sampleTimeSeconds == 1_000)
        #expect(row.source == "codex_app_server")
    }

    @Test("a refresh failure keeps all previous windows and affects only its provider")
    func failedRefresh() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        var claude = meter(usedPct: 40)
        claude.provider = "claude"
        claude.source = "yelo"
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 30), claude]), generation: generation)
        model.failUsage(provider: "codex", message: "Network read failed", generation: generation)
        let row = model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0]
        #expect(row.remainingPct == 70)
        #expect(row.status == "Usage unavailable")
        #expect(row.reason == "Network read failed")
        #expect(row.isOld)
        #expect(model.usageRows(provider: "claude", account: "work", nowSeconds: 1_000)[0].status == nil)
    }

    @Test("stale, unknown and missing sample time show uncertainty")
    func staleQuota() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 30, state: .stale)]), generation: generation)
        #expect(model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0].isOld)
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 30)]), generation: generation)
        #expect(!model.usageRows(provider: "codex", account: "work", nowSeconds: 1_300)[0].isOld)
        #expect(model.usageRows(provider: "codex", account: "work", nowSeconds: 1_301)[0].isOld)
        var unknown = meter(usedPct: 30, state: .unavailable)
        unknown.asOfSeconds = nil
        model.receiveUsage(SwarmUsage(meters: [unknown]), generation: generation)
        let row = model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)[0]
        #expect(row.status == "Usage unavailable")
        #expect(row.remainingPct == 70)
        #expect(row.isOld)
    }

    @Test("pane opened stays pending until a later status read proves sign-in")
    func loginPending() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        model.receiveAccounts(list(), provider: "codex", generation: generation)
        let request = SwarmAccountLoginRequest(provider: "codex", name: "work", revision: "opaque")
        let opened = try wireLogin()
        try model.loginOpened(opened, request: request)
        #expect(model.section("codex").pendingLogin == "work")
        model.receiveAccounts(list(), provider: "codex", generation: generation)
        #expect(model.section("codex").pendingLogin == "work")
        model.finishLoad(generation)
        let next = try start(&model)
        var unsigned = list()
        unsigned.accounts[0].authState = .signedOut
        model.receiveAccounts(unsigned, provider: "codex", generation: next)
        #expect(model.section("codex").pendingLogin == "work")
        unsigned.accounts[0].authState = .unavailable
        model.receiveAccounts(unsigned, provider: "codex", generation: next)
        #expect(model.section("codex").pendingLogin == "work")
        model.receiveAccounts(list(), provider: "codex", generation: next)
        #expect(model.section("codex").pendingLogin == nil)
    }

    @Test("an unknown or mismatched login result cannot claim an opened pane")
    func invalidLogin() throws {
        var model = AccountsPageModel()
        let request = SwarmAccountLoginRequest(provider: "codex", name: "work", revision: "opaque")
        var reply = try wireLogin()
        reply.state = .unavailable
        #expect(throws: SwarmProfileError.self) { try model.loginOpened(reply, request: request) }
        reply.state = .opened
        reply.account = "personal"
        #expect(throws: SwarmProfileError.self) { try model.loginOpened(reply, request: request) }
        #expect(model.section("codex").pendingLogin == nil)
    }

    @Test("metadata reload carries the latest revision and Reset names only metadata")
    func metadata() throws {
        var model = AccountsPageModel()
        var changed = list()
        changed.modified = true
        let generation = try start(&model)
        model.receiveAccounts(changed, provider: "codex", generation: generation)
        #expect(model.modified)
        #expect(model.revision == "opaque")
        model.finishLoad(generation)
        let reload = try start(&model)
        changed.revision = "conflict-reloaded"
        model.receiveAccounts(changed, provider: "codex", generation: reload)
        #expect(model.revision == "conflict-reloaded")
        #expect(AccountsPageModel.resetConfirmation.contains("Swarm account metadata"))
        #expect(AccountsPageModel.resetConfirmation.contains("CLI accounts stay signed in."))
    }

    @Test("all auth states have explicit row text")
    func authLabels() {
        #expect(AccountsPageModel.authLabel(.signedIn) == "Signed in")
        #expect(AccountsPageModel.authLabel(.signedOut) == "Not signed in")
        #expect(AccountsPageModel.authLabel(.unavailable) == "Status unavailable")
    }

    @Test("queued and running refreshes coalesce and page exit invalidates both")
    func refreshRequests() throws {
        var model = AccountsPageModel()
        let requested = model.requestLoad(refreshUsage: true)
        let duplicate = model.requestLoad(refreshUsage: true)
        #expect(requested)
        #expect(!duplicate)
        #expect(model.loadRequest == 1)
        #expect(model.shouldRefreshUsage)
        let generation = try start(&model)
        // The explicit request is consumed; re-entry cannot silently start another network refresh.
        #expect(!model.shouldRefreshUsage)
        let runningDuplicate = model.requestLoad(refreshUsage: true)
        #expect(!runningDuplicate)
        model.invalidateReads()
        model.receiveAccounts(list(), provider: "codex", generation: generation)
        #expect(model.revision == nil)
        let returned = model.requestLoad(refreshUsage: false)
        #expect(returned)
        #expect(!model.shouldRefreshUsage)
    }

    @Test("failed login and metadata conflict keep their message across revision reload")
    func actionFailures() throws {
        var model = AccountsPageModel()
        model.setActionError("The accounts file changed. Reload and retry.")
        let generation = try start(&model)
        var changed = list()
        changed.revision = "reloaded"
        model.receiveAccounts(changed, provider: "codex", generation: generation)
        #expect(model.errorMessage == "The accounts file changed. Reload and retry.")
        #expect(model.revision == "reloaded")
        #expect(model.section("codex").pendingLogin == nil)
        model.setActionError(nil)
        #expect(model.errorMessage == nil)
    }

    @Test("an account-wide failure keeps every old window and reports the read error")
    func accountWideFailure() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        var secondary = meter(usedPct: 60)
        secondary.window = "secondary"
        model.receiveUsage(SwarmUsage(meters: [meter(usedPct: 30), secondary]), generation: generation)
        var failed = meter(usedPct: nil, state: .failed)
        failed.window = nil
        failed.reason = "Quota source failed"
        model.receiveUsage(SwarmUsage(meters: [failed]), generation: generation, provider: "codex")
        let rows = model.usageRows(provider: "codex", account: "work", nowSeconds: 1_000)
        #expect(rows.map(\.remainingPct) == [70, 40])
        #expect(rows.allSatisfy { $0.isOld })
        #expect(model.errorMessage?.contains("Quota source failed") == true)
    }

    @Test("a payload for another provider fails its own section")
    func wrongProvider() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        model.receiveAccounts(list(), provider: "claude", generation: generation)
        #expect(model.section("claude").message == "Accounts unavailable")
        #expect(model.section("codex").message == "Reading accounts…")
        #expect(model.revision == nil)
    }

    @Test("Reset returns the new revision and clears only the Modified marker")
    func resetResult() throws {
        var model = AccountsPageModel()
        var changed = list()
        changed.modified = true
        let generation = try start(&model)
        model.receiveAccounts(changed, provider: "codex", generation: generation)
        let result = try JSONDecoder().decode(SwarmAccountMetadataAction.self, from: Data(#"{"revision":"bundled"}"#.utf8))
        model.metadataReset(result)
        #expect(!model.modified)
        #expect(model.revision == "bundled")
        #expect(model.section("codex").accounts[0].authState == .signedIn)
        #expect(model.section("codex").accounts[0].home == "/tmp/work")
    }

    @Test("one failed account does not mark another account's current reading old")
    func accountLocalFailure() throws {
        var model = AccountsPageModel()
        let generation = try start(&model)
        var failed = meter(usedPct: nil, state: .failed)
        failed.reason = "Work quota failed"
        var personal = meter(usedPct: 25)
        personal.account = "personal"
        model.receiveUsage(SwarmUsage(meters: [failed, personal]), generation: generation)
        let row = model.usageRows(provider: "codex", account: "personal", nowSeconds: 1_000)[0]
        #expect(row.status == nil)
        #expect(row.remainingPct == 75)
        #expect(!row.isOld)
        #expect(row.reason == nil)
        #expect(model.errorMessage?.contains("Work quota failed") == true)
    }

    @Test("failed pane open reloads the revision and permits the same-name typed retry")
    func loginRetry() async throws {
        let initial = list()
        let fixture = LoginRetryFixture(list: initial)
        let source = SwarmCLIProfileSource(environment: [:], cwd: "/tmp") { _, args, _, _ in
            try await fixture.run(args)
        }
        var model = AccountsPageModel()
        let generation = try start(&model)
        model.receiveAccounts(initial, provider: "codex", generation: generation)
        model.finishLoad(generation)
        let request = try #require(model.loginRequest(provider: "codex", name: "work"))
        await #expect(throws: SwarmProfileError.self) { try await source.openLogin(request) }
        model.setActionError("Cannot open login pane")
        let queued = model.requestLoad(refreshUsage: false)
        #expect(queued)
        let reload = try start(&model)
        let changed = try await source.accounts(provider: "codex")
        model.receiveAccounts(changed, provider: "codex", generation: reload)
        model.finishLoad(reload)
        let retry = try #require(model.loginRequest(provider: "codex", name: request.name))
        #expect(retry.name == request.name)
        #expect(retry.revision == "registered")
        model.setActionError(nil)
        let opened = try await source.openLogin(retry)
        try model.loginOpened(opened, request: retry)
        #expect(model.section("codex").pendingLogin == "work")
        #expect(model.errorMessage == nil)
        #expect(await fixture.loginCalls == 2)
        #expect(model.loginRequest(provider: "agy", name: "work") == nil)
        #expect(model.loginRequest(provider: "codex", name: "../work") == nil)
    }

    private func start(_ model: inout AccountsPageModel) throws -> Int {
        let generation = model.beginLoad()
        return try #require(generation)
    }

    private func list(accounts: [SwarmAccount]? = nil) -> SwarmAccountList {
        SwarmAccountList(provider: "codex", source: "swarm", accounts: accounts ?? [
            SwarmAccount(name: "work", email: "work@example.test", home: "/tmp/work",
                         env: ["CODEX_HOME": "/tmp/work"], authState: .signedIn, remainingPct: 70,
                         summary: "70% left", usageState: .fresh, usageSource: "codex_app_server"),
        ], auto: "work", revision: "opaque")
    }

    private func meter(usedPct: Double?, state: SwarmUsageState = .fresh) -> SwarmUsageMeter {
        SwarmUsageMeter(provider: "codex", account: "work", label: "work", window: "primary",
                        windowMinutes: 300, usedPct: usedPct, resetTimeSeconds: 1_500,
                        state: state, source: "codex_app_server", asOfSeconds: 1_000)
    }

    private func wireLogin() throws -> SwarmAccountLoginResult {
        try JSONDecoder().decode(SwarmAccountLoginResult.self, from: Data(
            #"{"provider":"codex","account":"work","pane":"pane-work","state":"opened","revision":"next"}"#.utf8
        ))
    }
}

private actor LoginRetryFixture {
    private var list: SwarmAccountList
    private(set) var loginCalls = 0

    init(list: SwarmAccountList) { self.list = list }

    func run(_ args: [String]) throws -> ShellResult {
        if args == ["accounts", "--provider", "codex", "--json"] {
            let encoder = JSONEncoder()
            encoder.keyEncodingStrategy = .convertToSnakeCase
            return ShellResult(status: 0, stdout: String(decoding: try encoder.encode(list), as: UTF8.self), stderr: "")
        }
        #expect(args == ["accounts", "login", "--provider", "codex", "--name", "work", "--revision", list.revision, "--json"])
        loginCalls += 1
        if loginCalls == 1 {
            list.revision = "registered"
            return ShellResult(status: 1, stdout: "", stderr: "Cannot open login pane")
        }
        return ShellResult(status: 0, stdout: #"{"provider":"codex","account":"work","pane":"pane-work","state":"opened","revision":"registered"}"#, stderr: "")
    }
}
