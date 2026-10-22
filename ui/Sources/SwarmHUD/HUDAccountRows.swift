// Account rows, reading text and usage cells for the open card.
import SwarmHUDCore
import SwiftUI

/// The account name.
struct AccountName: View {
    let account: UsageAccount
    let color: Color
    /// A one-line row has no room for the resets line; the name's tooltip carries it.
    var dense = false

    var body: some View {
        // The mark names the provider, so the name stands alone.
        VStack(alignment: .leading, spacing: 2) {
            Text(account.name).font(.system(size: 13, weight: .medium)).foregroundStyle(color)
                .lineLimit(1).truncationMode(.middle)
                .help(denseName)
                .accessibilityLabel(denseName)
            // A Codex account's resets sit under its name, where the row has a free second line.
            if !dense, let credits = account.resetCredits, let short = credits.short() {
                let full = credits.text() ?? short
                Text(short).font(.system(size: 11)).foregroundStyle(Palette.tertiary).lineLimit(1)
                    .help("\(full)\nRedeem one in ChatGPT settings.")
                    .accessibilityLabel(full)
            }
        }
        .frame(width: Layout.nameColumn, alignment: .leading)
    }

    /// A dense row's name also carries its resets, for the tooltip and VoiceOver.
    private var denseName: String {
        dense ? [account.name, account.resetCredits?.text()].compactMap { $0 }.joined(separator: ", ") : account.name
    }
}

struct AccountRow: View {
    let account: UsageAccount
    let kinds: [WindowKind]
    let dense: Bool
    let theme: Theme
    let now: Date
    /// False when every row reads the same; the status line then says it once.
    let showsReading: Bool
    let history: [HistorySample]
    /// The last read failed, so every number on screen is at least as old as `readAt`.
    let readFailed: Bool
    let readAt: Date?

    var body: some View {
        HStack(spacing: Layout.gap) {
            // The mark's label is the only place the row names its provider.
            ProviderMark(provider: account.provider, size: 16).frame(width: Layout.markColumn)
            AccountName(account: account, color: Palette.primary, dense: dense)
            if showsReading {
                reading.font(.system(size: 11)).lineLimit(1).frame(width: Layout.readingColumn, alignment: .leading)
            }
            ForEach(kinds, id: \.self) { kind in
                if let window = account.windows.first(where: { $0.kind == kind }) {
                    WindowCell(window: window, pace: pace(account: account, window: window, history: history, now: now),
                               theme: theme, now: now, dense: dense)
                } else {
                    // The column is shared, so this provider may have no such limit, or its read
                    // failed; "reading" is true for both.
                    Text("—").font(.system(size: 12)).foregroundStyle(Palette.tertiary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .help("No \(kind.title) reading")
                        .accessibilityLabel("\(kind.title) no reading")
                }
            }
        }
        .accessibilityElement(children: .combine)
        .frame(height: Layout.row(dense: dense))
    }

    @ViewBuilder private var reading: some View {
        if let stale = readingText(account, readFailed: readFailed, readAt: readAt, now: now) {
            Text(stale).foregroundStyle(Palette.warn)
        } else {
            Text("fresh").foregroundStyle(Palette.tertiary)
        }
    }
}

/// "Stale · 45m" for an account with an old window, nil when every window is fresh. Like the closed
/// wings' `~`, a failed read marks even fresh windows as old.
func readingText(_ account: UsageAccount, readFailed: Bool, readAt: Date?, now: Date) -> String? {
    guard readFailed || account.windows.contains(where: { !$0.isFresh }) else { return nil }
    let ages = account.windows.filter { readFailed || !$0.isFresh }.compactMap(\.asOf) + (readFailed ? [readAt].compactMap { $0 } : [])
    guard let oldest = ages.min() else { return "Stale" }
    // shortDuration says "now" under a minute, which reads wrong for an age.
    let age = now.timeIntervalSince(oldest) < 60 ? "<1m" : shortDuration(until: now, now: oldest)
    return "Stale · \(age)"
}

struct WindowCell: View {
    let window: UsageWindow
    /// nil while there are too few samples; the second line then stays blank.
    let pace: Pace?
    let theme: Theme
    let now: Date
    /// One line: a warning pace takes the reset's place.
    let dense: Bool

    private var paceText: String? {
        switch pace {
        case .full: return "full now"
        case .fillsIn(let seconds): return "full in \(fillDuration(seconds, now: now))"
        case .lasts: return "lasts to reset"
        case nil: return nil
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            usage
            // A blank line keeps the first line where it is in every row.
            if !dense {
                Text(paceText ?? " ")
                    .font(.system(size: 11).monospacedDigit())
                    .foregroundStyle(paceColor)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(window.kind.title) \(window.usedPct) percent used"
            + (window.severity == .critical ? ", critical" : "")
            + (window.resetAt.map { ", resets in \(shortDuration(until: $0, now: now))" } ?? "")
            + (paceText.map { ", \($0)" } ?? ""))
    }

    private var paceColor: Color {
        pace.map { $0 == .lasts ? Palette.tertiary : Palette.ink($0.severity(window: window)) } ?? Palette.tertiary
    }

    /// In a one-line row a pace that warns replaces the reset, as "full 3h".
    private var densePace: String? {
        guard dense, let pace, pace != .lasts, let text = paceText else { return nil }
        return text.replacingOccurrences(of: "full in ", with: "full ")
    }

    private var usage: some View {
        HStack(spacing: 8) {
            UsageBar(fraction: Double(window.usedPct) / 100, color: theme.fill(window.severity))
            Text("\(window.usedPct)%")
                .font(.system(size: 12, weight: .semibold).monospacedDigit())
                .foregroundStyle(Palette.ink(window.severity))
                .overlay(alignment: .topTrailing) {
                    if window.severity == .critical {
                        Text("!").font(.system(size: 8, weight: .heavy)).foregroundStyle(Palette.critical).offset(x: 6, y: -3)
                    }
                }
                .frame(width: Layout.valueColumn, alignment: .trailing)
            Text(densePace ?? window.resetAt.map { shortDuration(until: $0, now: now) } ?? "—")
                .font(.system(size: 12).monospacedDigit())
                .foregroundStyle(densePace == nil ? Palette.tertiary : paceColor)
                .lineLimit(1)
                .frame(width: Layout.resetColumn, alignment: .trailing)
        }
    }
}

/// A 6 pt bar with a tick at 80%, where the warning colour starts. Zero draws no fill.
struct UsageBar: View {
    let fraction: Double
    let color: Color

    var body: some View {
        GeometryReader { bar in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.track)
                if fraction > 0 {
                    Capsule().fill(color).frame(width: max(bar.size.width * min(fraction, 1), 6))
                }
                // Black on the fill, light gray on the bare track, so the mark shows either way.
                Rectangle().fill(fraction >= 0.8 ? Color.black.opacity(0.9) : Color.white.opacity(0.45))
                    .frame(width: 1).offset(x: bar.size.width * 0.8)
            }
        }
        .frame(height: 6)
    }
}

/// Every account with no reading on one line, so three expired tokens cost one row, not three.
struct SignedOutLine: View {
    let accounts: [UsageAccount]
    let renewing: Set<String>
    let onRenew: (UsageAccount) -> Void

    var body: some View {
        let renewable = accounts.filter { $0.renewalArguments != nil }
        let names = accounts.map(\.name).joined(separator: ", ")
        HStack(spacing: 10) {
            if Set(accounts.map(\.provider)).count == 1 { ProviderMark(provider: accounts[0].provider, size: 14) }
            Text(renewable.count == accounts.count
                 ? "\(accounts.count) \(accounts.count == 1 ? "account needs" : "accounts need") sign-in: \(names)"
                 : "No reading: \(names)")
                .lineLimit(1).truncationMode(.tail)
                .help(accounts.map { "\($0.name): \($0.problem ?? "No reading")" }.joined(separator: "\n"))
            if !renewable.isEmpty {
                if renewable.contains(where: { renewing.contains($0.id) }) {
                    ProgressView().controlSize(.mini).accessibilityLabel("Renewing tokens")
                } else {
                    Button(renewable.count == 1 ? "Renew" : "Renew all") { renewable.forEach(onRenew) }
                        .buttonStyle(.plain)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Palette.warn)
                        .underline()
                        .help("Run Claude on \(renewable.map(\.name).joined(separator: ", ")) in the background to renew the tokens, then refresh")
                        .fixedSize()
                }
            }
            Spacer(minLength: 0)
        }
        .font(.system(size: 12))
        .foregroundStyle(Palette.secondary)
        .padding(.horizontal, 10)
        .frame(height: Layout.needs - 8)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(hex: 0x141416)))
        .padding(.top, 8)
    }
}
