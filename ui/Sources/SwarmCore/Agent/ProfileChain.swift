import Foundation

/// How one runner shows in a profile's chain, from the last launch check.
public enum RunnerChipState: Sendable, Equatable {
    /// Runner 1 runs, or the check is not back: no mark, so healthy rows stay quiet.
    case normal
    /// A fallback the next launch takes.
    case next
    /// Skipped, with a short reason for the chip and the full one for the hover help.
    case skipped(short: String, full: String)

    public static func states(runners: Int, check: SwarmProfileCheck?) -> [RunnerChipState] {
        (0..<runners).map { index in
            if let skip = check?.skipped.first(where: { $0.index == index }) {
                return .skipped(short: shortReason(skip), full: skip.text)
            }
            return check?.pick == index && index > 0 ? .next : .normal
        }
    }

    /// "usage 2% left (threshold 5%)" becomes "usage 2% left"; other codes get a fixed word.
    static func shortReason(_ skip: SwarmSkip) -> String {
        switch skip.code {
        case "cli_missing": "CLI missing"
        case "signed_out": "signed out"
        default: skip.text.components(separatedBy: " (").first ?? skip.text
        }
    }
}

/// Which chips of a chain show when only `limit` fit: cut from the end, but never the chip the
/// next launch takes. A cut before it shows as "…" at the start.
public struct ChainFit: Sendable, Equatable {
    public let shown: [Int]
    public let leadingCut: Bool
    public let hidden: Int

    public init(count: Int, pick: Int?, limit: Int) {
        let limit = max(1, min(limit, count))
        let pick = pick.map { min(max($0, 0), count - 1) } ?? 0
        let start = max(0, pick - limit + 1)
        shown = Array(start..<(start + limit))
        leadingCut = start > 0
        hidden = count - limit
    }
}

/// Profiles grouped by the name before the first dot, `chat` first, the rest by name.
public struct ProfileGroup: Sendable, Equatable, Identifiable {
    public let name: String
    public let profiles: [SwarmProfile]
    public var id: String { name }

    public static func groups(_ profiles: [SwarmProfile]) -> [ProfileGroup] {
        let prefix = { (profile: SwarmProfile) in
            String(profile.name.split(separator: ".", maxSplits: 1).first ?? "")
        }
        var order: [String] = []
        var members: [String: [SwarmProfile]] = [:]
        for profile in profiles {
            let key = prefix(profile)
            if members[key] == nil { order.append(key) }
            members[key, default: []].append(profile)
        }
        let sorted = order.sorted { lhs, rhs in
            lhs == "chat" || (rhs != "chat" && lhs < rhs)
        }
        return sorted.map { ProfileGroup(name: $0, profiles: members[$0] ?? []) }
    }
}

/// The counts in the page's health line.
public struct ProfileHealth: Sendable, Equatable {
    public var ready = 0
    public var fallback = 0
    public var blocked = 0

    public init(_ statuses: [ProfileStatus?]) {
        for status in statuses {
            switch status?.kind {
            case .primary: ready += 1
            case .fallback: fallback += 1
            case .none?: blocked += 1
            case nil: break
            }
        }
    }
}

extension ProfileStatus {
    /// The pill text.
    public var title: String {
        switch kind {
        case .primary: "Ready"
        case .fallback: "On fallback"
        case .none: "Blocked"
        }
    }
}

/// The status line of one runner card in the editor. A check covers only the saved profile, so
/// once the draft differs, the card says so rather than show a stale answer.
public enum RunnerCardStatus: Sendable, Equatable {
    case unknown
    case next
    case standby
    case skipped(String)
    case added
    case changed
    case saveToUpdate
}

extension ProfileDraft {
    public func cardStatus(at index: Int, check: SwarmProfileCheck?) -> RunnerCardStatus {
        guard runners.indices.contains(index) else { return .unknown }
        let runner = runners[index]
        guard let saved = original.runners.first(where: { $0.id == runner.id }) else { return .added }
        if saved != runner { return .changed }
        guard runners == original.runners else { return .saveToUpdate }
        guard let check else { return .unknown }
        if let skip = check.skipped.first(where: { $0.index == index }) { return .skipped(skip.text) }
        return check.pick == index ? .next : .standby
    }
}
