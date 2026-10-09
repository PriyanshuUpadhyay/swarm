import Foundation

public enum AccountsPageError: LocalizedError, Sendable {
    case cliMissing(String)

    public var errorDescription: String? {
        switch self {
        case .cliMissing(let provider): "Install the \(AccountsPageModel.providerTitle(provider)) CLI to add an account."
        }
    }
}

public struct AccountsProviderSection: Sendable, Hashable, Identifiable {
    public var id: String { provider }
    public let provider: String
    public let title: String
    public let source: String?
    public let accounts: [SwarmAccount]
    public let auto: String?
    public let message: String?
    public let error: String?
    public let canAdd: Bool
    public let pendingLogin: String?
    public var usageMessage: String? {
        provider == "agy" ? "No usage source in Swarm. View quota with /usage in the CLI." : nil
    }
}

public struct AccountsUsageRow: Sendable, Hashable, Identifiable {
    public let id: String
    public let title: String
    public let remainingPct: Double?
    public let status: String?
    public let reason: String?
    public let isOld: Bool
    public let source: String?
    public let sampleTimeSeconds: Int?
    public let resetTimeSeconds: Int?
}

/// Owns page decisions without UI, provider processes, or clocks hidden inside state transitions.
public struct AccountsPageModel: Sendable, Equatable {
    private static let providers = ["claude", "codex", "agy"]
    public static let resetConfirmation = "Reset to bundled clears Swarm account metadata. CLI accounts stay signed in."
    public private(set) var generation = 0
    public private(set) var isLoading = false
    public private(set) var loadRequest = 0
    public private(set) var shouldRefreshUsage = false
    public private(set) var hasPendingRead = false
    private var actionError: String?
    public private(set) var revision: String?
    public private(set) var modified = false
    private var lists: [String: SwarmAccountList] = [:]
    private var readingProviders = Set(providers)
    private var accountErrors: [String: String] = [:]
    private var missingCLIs: Set<String> = []
    private var usageErrors: [String: String] = [:]
    private var meters: [SwarmUsageMeter] = []
    private var pendingLogins: [String: PendingLogin] = [:]

    private struct PendingLogin: Sendable, Equatable {
        let name: String
        let generation: Int
    }

    public init() {}

    public var sections: [AccountsProviderSection] { Self.providers.map(section) }

    public var errorMessage: String? {
        let messages = [actionError].compactMap { $0 } + Self.providers.flatMap { provider in
            [accountErrors[provider], usageErrors[provider]].compactMap { $0 }.map {
                "\(Self.providerTitle(provider)): \($0)"
            }
        }
        let quotaFailures = meters.filter { [.failed, .unavailable].contains($0.state) }.map {
            "\(Self.providerTitle($0.provider)): \($0.account ?? $0.label): \($0.reason ?? "Usage status is unavailable")"
        }
        var unique: [String] = []
        for message in messages + quotaFailures where !unique.contains(message) { unique.append(message) }
        return unique.isEmpty ? nil : unique.joined(separator: "\n")
    }

    fileprivate static func providerTitle(_ provider: String) -> String {
        switch provider {
        case "claude": "Claude"
        case "codex": "Codex"
        case "agy": "AGY"
        default: provider
        }
    }

    public static func authLabel(_ state: SwarmAccountAuthState) -> String {
        switch state {
        case .signedIn: "Signed in"
        case .signedOut: "Not signed in"
        case .unavailable: "Status unavailable"
        }
    }

    public func section(_ provider: String) -> AccountsProviderSection {
        let list = lists[provider]
        let message: String?
        if missingCLIs.contains(provider) {
            message = "Install the \(Self.providerTitle(provider)) CLI to add an account."
        } else if accountErrors[provider] != nil || list?.state == .unavailable {
            message = "Accounts unavailable"
        } else if list?.state == .noSource {
            message = "No account source in Swarm."
        } else if list == nil && readingProviders.contains(provider) {
            message = "Reading accounts…"
        } else if list?.accounts.isEmpty != false {
            message = "No accounts yet."
        } else {
            message = nil
        }
        let ready = list?.state == .ready && accountErrors[provider] == nil && !missingCLIs.contains(provider)
        return AccountsProviderSection(
            provider: provider, title: Self.providerTitle(provider), source: list?.source,
            accounts: ready ? list?.accounts ?? [] : [], auto: ready ? list?.auto : nil,
            message: message, error: accountErrors[provider], canAdd: ready && provider != "agy",
            pendingLogin: pendingLogins[provider]?.name
        )
    }

    /// SwiftUI uses this request as its task identity, so page exit cancels only reads.
    @discardableResult
    public mutating func requestLoad(refreshUsage: Bool) -> Bool {
        guard !isLoading, !hasPendingRead else { return false }
        shouldRefreshUsage = refreshUsage
        hasPendingRead = true
        loadRequest += 1
        return true
    }

    public mutating func setActionError(_ message: String?) {
        actionError = message
    }

    public mutating func metadataReset(_ result: SwarmAccountMetadataAction) {
        revision = result.revision
        modified = false
    }

    /// One visible refresh covers repeated clicks until it finishes.
    public mutating func beginLoad() -> Int? {
        guard !isLoading else { return nil }
        generation += 1
        isLoading = true
        shouldRefreshUsage = false
        hasPendingRead = false
        readingProviders = Set(Self.providers)
        return generation
    }

    public mutating func invalidateReads() {
        generation += 1
        isLoading = false
        hasPendingRead = false
        readingProviders = []
    }

    public mutating func finishLoad(_ generation: Int) {
        guard generation == self.generation else { return }
        isLoading = false
        hasPendingRead = false
        readingProviders = []
    }

    public mutating func receiveAccounts(_ list: SwarmAccountList, provider: String, generation: Int) {
        guard generation == self.generation else { return }
        guard list.provider == provider else {
            failAccounts(provider: provider, error: SwarmProfileError.failed("swarm returned a different provider"), generation: generation)
            return
        }
        lists[provider] = list
        readingProviders.remove(provider)
        missingCLIs.remove(provider)
        accountErrors[provider] = list.state == .unavailable ? "Account status is unavailable" : nil
        revision = list.revision
        modified = list.modified
        if let pending = pendingLogins[provider], generation > pending.generation,
           list.state == .ready,
           list.accounts.contains(where: { $0.name == pending.name && $0.authState == .signedIn }) {
            pendingLogins[provider] = nil
        }
    }

    public mutating func failAccounts(provider: String, error: any Error, generation: Int) {
        guard generation == self.generation else { return }
        readingProviders.remove(provider)
        accountErrors[provider] = Self.message(error)
        if case AccountsPageError.cliMissing = error { missingCLIs.insert(provider) }
    }

    public mutating func receiveUsage(_ usage: SwarmUsage, generation: Int, provider: String? = nil) {
        guard generation == self.generation else { return }
        for selected in provider.map({ [$0] }) ?? Self.providers {
            let incoming = usage.meters.filter { $0.provider == selected }
            let previous = meters.filter { $0.provider == selected }
            let replacement = incoming.flatMap { meter -> [SwarmUsageMeter] in
                guard [.failed, .unavailable].contains(meter.state), meter.usedPct == nil else { return [meter] }
                let old = previous.filter { $0.account == meter.account && (meter.window == nil || $0.id == meter.id) && $0.usedPct != nil }
                guard !old.isEmpty else { return [meter] }
                return old.map { sample in
                    var kept = sample
                    kept.state = meter.state
                    kept.reason = meter.reason
                    return kept
                }
            }
            meters.removeAll { $0.provider == selected }
            meters.append(contentsOf: replacement)
            usageErrors[selected] = nil
        }
    }

    public mutating func failUsage(provider: String?, message: String, generation: Int) {
        guard generation == self.generation else { return }
        for selected in provider.map({ [$0] }) ?? ["claude", "codex"] {
            usageErrors[selected] = message
        }
    }

    public mutating func loginOpened(_ result: SwarmAccountLoginResult, request: SwarmAccountLoginRequest) throws {
        guard result.state == .opened, result.provider == request.provider,
              result.account == request.name, !result.pane.isEmpty else {
            throw SwarmProfileError.failed("swarm did not confirm the requested login pane")
        }
        revision = result.revision
        pendingLogins[request.provider] = PendingLogin(name: request.name, generation: generation)
    }

    public func usageRows(provider: String, account: String, nowSeconds: Int) -> [AccountsUsageRow] {
        let samples = meters.filter { $0.provider == provider && $0.account == account }
        if samples.isEmpty {
            let native = lists[provider]?.accounts.first { $0.name == account }
            let state = usageErrors[provider] == nil ? native?.usageState ?? .missing : .failed
            return [AccountsUsageRow(
                id: "\(provider):\(account):missing", title: "", remainingPct: nil,
                status: Self.usageStatus(state, percentage: nil), reason: usageErrors[provider], isOld: false,
                source: native?.usageSource, sampleTimeSeconds: nil, resetTimeSeconds: nil
            )]
        }
        return samples.map { meter in
            let state = usageErrors[provider] == nil ? meter.state : .failed
            let percentage = [.missing, .noSource].contains(state) ? nil : meter.usedPct.map { 100 - $0 }
            let ageIsOld = meter.asOfSeconds.map { nowSeconds - $0 > 300 || $0 > nowSeconds } ?? true
            let isOld = percentage != nil && (state != .fresh || ageIsOld)
            let title = [meter.window, meter.windowMinutes.map { "\($0) min" }].compactMap { $0 }.joined(separator: " · ")
            return AccountsUsageRow(
                id: meter.id, title: title, remainingPct: percentage,
                status: Self.usageStatus(state, percentage: percentage), reason: usageErrors[provider] ?? meter.reason,
                isOld: isOld, source: meter.source, sampleTimeSeconds: meter.asOfSeconds,
                resetTimeSeconds: meter.resetTimeSeconds
            )
        }
    }

    private static func usageStatus(_ state: SwarmUsageState, percentage: Double?) -> String? {
        switch state {
        case .fresh, .stale: percentage == nil ? "No usage reading" : nil
        case .missing: "No usage reading"
        case .failed, .unavailable: "Usage unavailable"
        case .noSource: "No usage source in Swarm"
        }
    }

    public static func message(_ error: any Error) -> String {
        (error as? SwarmProfileError)?.message ?? error.localizedDescription
    }
}
