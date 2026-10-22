// The open usage card: a header with the mascot and the answer, the running agents, one ledger
// table with a column per limit kind, and one line for accounts with no reading.
import SwarmHUDCore
import SwiftUI

struct OpenContent: View {
    let model: UsageModel
    let theme: Theme
    let live: Bool
    let now: Date

    var body: some View {
        let table = ledger(model.accounts)
        let showsReading = model.sharedReading(now: now) == nil
        let dense = Layout.isDense(rows: table.rows.count)
        VStack(spacing: 0) {
            status.frame(height: Layout.header(notch: model.notch))
            if !model.agents.agents.isEmpty {
                RunningLine(agents: model.agents.agents, theme: theme, shown: model.openSize.width > Layout.chipBreak ? Layout.wideChips : Layout.narrowChips).frame(height: Layout.running)
            }
            Spacer().frame(height: Layout.belowStatus)
            if !table.rows.isEmpty {
                headings(table.kinds, showsReading: showsReading)
                let rows = VStack(spacing: 0) {
                    // Account order is the reader's inside a provider, so a sign-in never moves a row.
                    ForEach(Array(table.rows.enumerated()), id: \.element.id) { index, account in
                        AccountRow(account: account, kinds: table.kinds, dense: dense, theme: theme, now: now,
                                   showsReading: showsReading,
                                   history: model.history, readFailed: model.readError != nil, readAt: model.readAt)
                            .padding(.horizontal, Layout.rowPadding)
                            .background(index.isMultiple(of: 2) ? Color.clear : theme.stripe)
                            .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
                    }
                }
                // The card stops at `maxOpenHeight`; past it the rows scroll. Only then, because
                // `--snapshot`'s ImageRenderer draws a ScrollView empty.
                ViewThatFits(in: .vertical) {
                    rows
                    ScrollView { rows }
                }
            }
            if !table.signedOut.isEmpty {
                SignedOutLine(accounts: table.signedOut, renewing: model.renewing, onRenew: model.renew)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Layout.inset)
    }

    /// One heading per limit kind, the same for every provider, so Week sits in one column.
    private func headings(_ kinds: [WindowKind], showsReading: Bool) -> some View {
        HStack(spacing: Layout.gap) {
            Spacer().frame(width: Layout.markColumn)
            Text("Account").frame(width: Layout.nameColumn, alignment: .leading)
            if showsReading {
                Text("Reading").frame(width: Layout.readingColumn, alignment: .leading)
            }
            ForEach(kinds, id: \.self) { kind in
                Text(columnTitle(kind)).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .font(.system(size: 11, weight: .medium))
        .foregroundStyle(Palette.tertiary)
        .padding(.horizontal, Layout.rowPadding)
        .frame(height: Layout.headings)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }

    private func columnTitle(_ kind: WindowKind) -> String {
        switch kind {
        case .session: return "Session · 5h"
        case .week: return "Week"
        case .fable: return "Fable week"
        }
    }

    /// The closed wing shows it on the left, so it stays on the left when the card opens. Who it is
    /// and who drew it shows on hover; the guests' licences ask for that credit.
    private var mascot: some View {
        let turn = model.mascotTurn(now: now)
        return model.mascot(now: now, height: Layout.openMascot, live: live)
            .frame(width: 1.6 * Layout.openMascot, height: Layout.openMascot)
            .help("\(MascotCast.name(turn)) · \(MascotCast.guest(turn)?.credit ?? "swarm's own art")")
    }

    /// The mascot and which account to use next left of the camera, the phase and read age right of it.
    private var status: some View {
        let wing = max((model.openSize.width - 2 * Layout.inset - model.notch.width) / 2 - 12, 0)
        let best = bestAccount(model.accounts, history: model.history, now: now)
        return HStack(spacing: 0) {
            HStack(spacing: 10) {
                mascot
                VStack(alignment: .leading, spacing: 3) {
                    if let best {
                        // A long name gives way; the room left always shows.
                        HStack(spacing: 0) {
                            Circle().fill(theme.fill(best.severity)).frame(width: 7, height: 7).padding(.trailing, 7)
                            Text("Use next ").foregroundStyle(Palette.tertiary).fixedSize()
                            if best.isOld(readFailed: model.readError != nil) { Text("~").fixedSize() }
                            // The mark names the provider, so a narrow card keeps room for the name.
                            ProviderMark(provider: best.account.provider, size: 12).padding(.trailing, 4)
                            Text(best.account.name).fontWeight(.semibold).lineLimit(1).truncationMode(.middle)
                                .layoutPriority(1)
                            Text(" · ").fixedSize()
                            Text(best.paceText(now: now)).fixedSize()
                                .foregroundStyle(best.severity == .calm ? Palette.primary : Palette.ink(best.severity))
                        }
                        .foregroundStyle(Palette.primary)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(best.spoken(now: now))
                        Text(roomText(best))
                            .font(.system(size: 11))
                            .foregroundStyle(Palette.tertiary)
                            .lineLimit(1).truncationMode(.tail)
                    } else {
                        // The cause names the fix (for example "swarm not found; set SWARM_BIN").
                        Text(model.readError.map { "\($0) · retries each minute" } ?? "No usage reading")
                            .foregroundStyle(Palette.secondary)
                            .lineLimit(1).truncationMode(.tail)
                            .help(model.readError ?? "")
                    }
                }
            }
            .frame(width: wing, alignment: .leading)
            Spacer(minLength: 0)
            HStack(spacing: 6) {
                Image(systemName: theme.symbol).font(.system(size: 11)).accessibilityHidden(true)
                Text(model.phase.title)
                Text("·").accessibilityHidden(true)
                if let refreshError = model.refreshError {
                    Text(refreshError).foregroundStyle(Palette.warn).lineLimit(1).truncationMode(.tail)
                } else {
                    Text(age).monospacedDigit()
                }
                // Every row read the same, so the table drops its Reading column and says it here.
                if let shared = model.sharedReading(now: now), !shared.isEmpty {
                    Text("·").accessibilityHidden(true)
                    Text("all \(shared.prefix(1).lowercased() + shared.dropFirst())").foregroundStyle(Palette.warn).monospacedDigit()
                }
                Button { model.refresh() } label: {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 11, weight: .semibold))
                        .rotationEffect(.degrees(model.refreshing.isEmpty ? 0 : 180))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(model.canRefresh ? Palette.secondary : Palette.tertiary.opacity(0.6))
                .disabled(!model.canRefresh)
                .help(model.refreshError ?? (model.canRefresh ? "Refresh usage" : "Next refresh in a few minutes"))
                .accessibilityLabel("Refresh usage")
            }
            .foregroundStyle(Palette.tertiary)
            .frame(width: wing, alignment: .trailing)
        }
        .font(.system(size: 12, weight: .medium))
    }

    /// The other provider's best account, since a limit on one provider leaves the other free.
    private func roomText(_ best: BestAccount) -> String {
        let others = model.accounts.filter { $0.provider != best.account.provider }
        // A full account is no room, so the line falls back to the count.
        if let other = bestAccount(others, history: model.history, now: now), other.fullest < 100 {
            return "Room on \(providerTitle(other.account.provider)) \(other.account.name): \(100 - other.fullest)% left"
        }
        let read = model.accounts.filter { !$0.windows.isEmpty }.count
        return "\(read) \(read == 1 ? "account" : "accounts") read"
    }

    private var age: String {
        if model.readError != nil { return "read failed" }
        guard let readAt = model.readAt else { return "reading…" }
        let minutes = Int(now.timeIntervalSince(readAt) / 60)
        return minutes < 1 ? "updated now" : "updated \(minutes)m ago"
    }
}


/// The agents swarm runs now, waiting ones first: `shown` chips, then "+N".
struct RunningLine: View {
    let agents: [RunningAgent]
    let theme: Theme
    let shown: Int

    var body: some View {
        HStack(spacing: 8) {
            Text("Running now").foregroundStyle(Palette.tertiary)
            ForEach(agents.prefix(shown), id: \.key) { agent in chip(agent) }
            if agents.count > shown {
                Text("+\(agents.count - shown)")
                    .padding(.horizontal, 9).padding(.vertical, 4)
                    .background(Capsule().fill(Palette.chip))
                    .help(agents.dropFirst(shown).map { "\($0.id) · \(modelLabel($0.model, provider: $0.provider))" }.joined(separator: "\n"))
            }
            Spacer(minLength: 0)
        }
        .font(.system(size: 12))
        .foregroundStyle(Palette.secondary)
        .padding(.horizontal, Layout.rowPadding)
    }

    private func chip(_ agent: RunningAgent) -> some View {
        HStack(spacing: 6) {
            Circle().fill(agent.waiting ? Palette.warn : theme.calm).frame(width: 6, height: 6)
            ProviderMark(provider: agent.provider, size: 12)
            // The name keeps its width; the model and account give way.
            Text(agent.id.count > 18 ? agent.id.prefix(17) + "…" : agent.id).fontWeight(.medium)
                .foregroundStyle(agent.waiting ? Palette.warn : Palette.primary)
                .fixedSize()
            // "needs you" first, so a crowded chip cuts the model, not the call to act.
            Text(((agent.waiting ? ["needs you"] : []) + [modelLabel(agent.model, provider: agent.provider)]
                  + [agent.account].compactMap { $0 }).joined(separator: " · "))
                .lineLimit(1)
                .foregroundStyle(agent.waiting ? Palette.warn : Palette.secondary)
        }
        .padding(.horizontal, 9).padding(.vertical, 4)
        .background(Capsule().fill(agent.waiting ? Palette.chipWaiting : Palette.chip))
        // Chips take the line before the trailing spacer does.
        .layoutPriority(1)
        // The tooltip has the whole id, which the chip may cut.
        .help("\(agent.id) · \(agent.model ?? providerTitle(agent.provider))")
        .accessibilityElement(children: .combine)
    }
}
