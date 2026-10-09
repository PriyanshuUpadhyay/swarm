import SwiftUI
import SwarmCore

struct AccountsPage: View {
    @Environment(\.designTokens) private var tokens
    @Environment(\.scenePhase) private var scenePhase
    let loadAccounts: (String) async throws -> SwarmAccountList
    let loadUsage: () async throws -> [SwarmUsageMeter]
    let refreshUsage: (String) async throws -> SwarmUsage
    let openLogin: (SwarmAccountLoginRequest) async throws -> SwarmAccountLoginResult
    let resetAccounts: (String) async throws -> SwarmAccountMetadataAction
    let onError: (String?) -> Void

    @State private var state = AccountsPageModel()
    @State private var addingProvider: String?
    @State private var accountName = ""
    @State private var isActing = false
    @State private var confirmReset = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: tokens.spacing.l) {
                header
                metadataControls
                TimelineView(.periodic(from: .now, by: 60)) { context in
                    VStack(alignment: .leading, spacing: tokens.spacing.l) {
                        ForEach(state.sections) { section in
                            providerSection(section, nowSeconds: Int(context.date.timeIntervalSince1970))
                            Divider()
                        }
                    }
                }
            }
            .padding(tokens.spacing.xl)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .task(id: state.loadRequest) { await load(explicitRefresh: state.shouldRefreshUsage) }
        .onDisappear { state.invalidateReads() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { state.requestLoad(refreshUsage: false) }
        }
    }

    private var busy: Bool { isActing || state.isLoading || state.hasPendingRead }

    private var header: some View {
        HStack {
            Text("Accounts").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
            Spacer()
            if state.isLoading { DelayedProgress("Reading accounts…") }
            Button("Refresh") {
                state.setActionError(nil)
                state.requestLoad(refreshUsage: true)
            }
            .disabled(busy)
            .help("Read account status and cached Claude usage. Refresh Codex usage.")
        }
    }

    private var metadataControls: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.s) {
            HStack {
                Text("Account settings").font(.headline)
                Text(state.modified ? "Modified" : "Bundled").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Reset to bundled") { confirmReset = true }
                    .disabled(busy || state.revision == nil || !state.modified)
            }
            if confirmReset {
                Text(AccountsPageModel.resetConfirmation)
                    .fixedSize(horizontal: false, vertical: true)
                HStack {
                    Button("Cancel") { confirmReset = false }
                    Button("Reset to bundled", role: .destructive) { reset() }
                }
                .disabled(busy)
            }
        }
    }

    private func providerSection(_ section: AccountsProviderSection, nowSeconds: Int) -> some View {
        VStack(alignment: .leading, spacing: tokens.spacing.m) {
            HStack(alignment: .firstTextBaseline) {
                Text(section.title).font(.title2.bold()).accessibilityAddTraits(.isHeader)
                Spacer()
                if section.provider != "agy" {
                    Button("Add account") {
                        addingProvider = section.provider
                        confirmReset = false
                    }
                    .disabled(!section.canAdd || busy)
                    .help(section.message ?? "Open the provider's login pane")
                }
            }
            if let source = section.source {
                Text("Account source: \(sourceLabel(source))").font(.caption).foregroundStyle(.secondary)
            }
            if let message = section.message {
                Text(verbatim: message).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let error = section.error, error != section.message {
                Text(verbatim: error).font(.callout).foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            }
            if let usageMessage = section.usageMessage {
                Text(verbatim: usageMessage).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if addingProvider == section.provider { nameForm(section) }
            if let name = section.pendingLogin {
                VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                    Text("Finish signing in in the login pane.")
                    Text("Waiting for \(name) to sign in.").font(.caption).foregroundStyle(.secondary)
                    Button("Refresh accounts") { state.requestLoad(refreshUsage: false) }
                        .disabled(busy)
                }
            }
            ForEach(section.accounts) { account in
                accountRow(account, section: section, nowSeconds: nowSeconds)
            }
        }
    }

    private func nameForm(_ section: AccountsProviderSection) -> some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            TextField("Account name", text: $accountName)
                .textFieldStyle(.roundedBorder)
                .onSubmit { login(section.provider) }
            Text("Use a–z, 0–9, '.', '_' or '-'. Auto and default are reserved.")
                .font(.caption).foregroundStyle(.secondary)
            if !accountName.isEmpty && !SwarmAccountLoginRequest.validName(accountName) {
                Text("Enter a valid account name without spaces or path parts.")
                    .font(.caption).foregroundStyle(.red)
            }
            HStack {
                Button("Open login") { login(section.provider) }
                    .disabled(busy || !section.canAdd || !SwarmAccountLoginRequest.validName(accountName))
                Button("Cancel") { addingProvider = nil }.disabled(isActing)
            }
        }
    }

    private func accountRow(_ account: SwarmAccount, section: AccountsProviderSection, nowSeconds: Int) -> some View {
        VStack(alignment: .leading, spacing: tokens.spacing.s) {
            HStack(alignment: .firstTextBaseline) {
                Text(account.name).font(.headline)
                Spacer()
                if section.auto == account.name { Label("Auto", systemImage: "sparkles").font(.callout) }
            }
            if let email = account.email {
                Text(verbatim: email).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }
            Text(AccountsPageModel.authLabel(account.authState)).foregroundStyle(.secondary)
            if let summary = account.summary {
                Text(verbatim: summary).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(state.usageRows(provider: section.provider, account: account.name, nowSeconds: nowSeconds)) { row in
                usageRow(row)
            }
        }
        .padding(tokens.spacing.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(DesignTokens.selectionFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.control))
        .accessibilityElement(children: .contain)
    }

    private func usageRow(_ row: AccountsUsageRow) -> some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            if !row.title.isEmpty { Text(verbatim: row.title).font(.callout) }
            if let status = row.status { Text(verbatim: status).foregroundStyle(.secondary) }
            if let reason = row.reason {
                Text(verbatim: reason).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
            }
            if let remaining = row.remainingPct {
                let value = "\(remaining.formatted(.number.precision(.fractionLength(0...1))))% left"
                ProgressView(value: remaining, total: 100)
                    .accessibilityLabel(row.title.isEmpty ? "Usage left" : row.title)
                    .accessibilityValue(value)
                Text(value)
                if row.isOld { Label("Old reading", systemImage: "clock").font(.caption) }
                if let reset = row.resetTimeSeconds {
                    Text("Resets \(date(reset))").font(.caption).foregroundStyle(.secondary)
                }
            }
            Text("Usage source: \(row.source.map(sourceLabel) ?? "Unavailable")")
                .font(.caption).foregroundStyle(.secondary)
            Text(row.sampleTimeSeconds.map { "Sample time: \(date($0))" } ?? "Sample time unavailable")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private func sourceLabel(_ source: String) -> String {
        switch source {
        case "swarm": "Swarm"
        case "codex_app_server": "Codex app-server"
        default: source
        }
    }

    private func date(_ seconds: Int) -> String {
        Date(timeIntervalSince1970: Double(seconds)).formatted(date: .abbreviated, time: .shortened)
    }

    private func load(explicitRefresh: Bool) async {
        guard !Task.isCancelled, let generation = state.beginLoad() else { return }
        defer { state.finishLoad(generation) }
        async let claude: Void = readAccounts("claude", generation: generation)
        async let codex: Void = readAccounts("codex", generation: generation)
        async let agy: Void = readAccounts("agy", generation: generation)
        do {
            let usage = try await loadUsage()
            try Task.checkCancellation()
            state.receiveUsage(SwarmUsage(meters: usage), generation: generation)
            reportError(generation)
        } catch is CancellationError {
            return
        } catch {
            state.failUsage(provider: nil, message: AccountsPageModel.message(error), generation: generation)
            reportError(generation)
        }
        // A cached read completes before explicit refresh, so it cannot overwrite the new sample.
        if explicitRefresh && !Task.isCancelled && generation == state.generation {
            do {
                let usage = try await refreshUsage("codex")
                try Task.checkCancellation()
                state.receiveUsage(usage, generation: generation, provider: "codex")
                reportError(generation)
            } catch is CancellationError {
                return
            } catch {
                state.failUsage(provider: "codex", message: AccountsPageModel.message(error), generation: generation)
                reportError(generation)
            }
        }
        _ = await (claude, codex, agy)
        if explicitRefresh && !Task.isCancelled && generation == state.generation {
            await readAccounts("codex", generation: generation)
        }
    }

    private func readAccounts(_ provider: String, generation: Int) async {
        do {
            let list = try await loadAccounts(provider)
            try Task.checkCancellation()
            state.receiveAccounts(list, provider: provider, generation: generation)
            reportError(generation)
        } catch is CancellationError {
            return
        } catch {
            state.failAccounts(provider: provider, error: error, generation: generation)
            reportError(generation)
        }
    }

    private func reportError(_ generation: Int) {
        if generation == state.generation { onError(state.errorMessage) }
    }

    private func login(_ provider: String) {
        guard !busy, state.section(provider).canAdd, SwarmAccountLoginRequest.validName(accountName),
              let revision = state.revision else { return }
        let request = SwarmAccountLoginRequest(provider: provider, name: accountName, revision: revision)
        isActing = true
        state.setActionError(nil)
        onError(state.errorMessage)
        Task {
            defer { isActing = false }
            do {
                let result = try await openLogin(request)
                try state.loginOpened(result, request: request)
                addingProvider = nil
                state.requestLoad(refreshUsage: false)
            } catch {
                state.setActionError(AccountsPageModel.message(error))
                // Reload revisions after any failure; the form input stays for retry.
                state.requestLoad(refreshUsage: false)
                onError(state.errorMessage)
            }
        }
    }

    private func reset() {
        guard !busy, let revision = state.revision else { return }
        isActing = true
        state.setActionError(nil)
        onError(state.errorMessage)
        Task {
            defer { isActing = false }
            do {
                let result = try await resetAccounts(revision)
                state.metadataReset(result)
                confirmReset = false
                state.requestLoad(refreshUsage: false)
            } catch {
                state.setActionError(AccountsPageModel.message(error))
                state.requestLoad(refreshUsage: false)
                onError(state.errorMessage)
            }
        }
    }
}
