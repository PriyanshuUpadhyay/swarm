// The open usage card, its provider tables, status line and mascot credit.
import SwarmHUDCore
import SwiftUI

struct OpenContent: View {
    let model: UsageModel
    let theme: Theme
    let live: Bool
    let now: Date

    var body: some View {
        let groups = providerGroups(model.accounts)
        // Every group draws as many columns as the widest one, so a column is as wide in each group.
        let columns = groups.map(\.kinds.count).max() ?? 0
        VStack(spacing: 0) {
            status.frame(height: Layout.band(notch: model.notch))
            Spacer().frame(height: Layout.belowStatus)
            ForEach(groups) { group in
                if model.footerBeside && group.id == groups.last?.id {
                    HStack(alignment: .top, spacing: 0) {
                        table(group, columns: group.kinds.count)
                            .frame(width: Layout.groupWidth(kinds: group.kinds.count, columns: columns,
                                                            reading: model.sharedReading(now: now) == nil))
                        VStack(alignment: .trailing, spacing: 6) {
                            Spacer(minLength: 0)
                            HStack(alignment: .bottom, spacing: 12) { credit; mascot }
                            legend.multilineTextAlignment(.trailing)
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(Palette.tertiary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .trailing)
                        .padding(.leading, Layout.gap)
                        .padding(.horizontal, Layout.rowPadding)
                        .padding(.bottom, 6)
                    }
                    .fixedSize(horizontal: false, vertical: true)
                } else {
                    table(group, columns: columns)
                }
            }
            if !model.footerBeside {
                HStack(spacing: 12) {
                    legend.frame(maxWidth: .infinity, alignment: .leading)
                    credit
                    mascot
                }
                .font(.system(size: 11))
                .foregroundStyle(Palette.tertiary)
                .frame(height: Layout.legend)
                .padding(.horizontal, Layout.rowPadding)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Layout.inset)
    }

    /// One provider's heading and rows, drawn `columns` wide so each column lines up across groups.
    private func table(_ group: ProviderGroup, columns: Int) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: Layout.gap) {
                Spacer().frame(width: Layout.markColumn)
                Text(providerTitle(group.provider)).frame(width: Layout.nameColumn, alignment: .leading)
                if model.sharedReading(now: now) == nil {
                    Text("Reading").frame(width: Layout.readingColumn, alignment: .leading)
                }
                ForEach(group.kinds, id: \.self) { kind in
                    Text(kind.title).frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(group.kinds.count..<columns, id: \.self) { _ in Color.clear.frame(maxWidth: .infinity) }
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(Palette.tertiary)
            .padding(.horizontal, Layout.rowPadding)
            .frame(height: Layout.headings)
            // Each row's label names its windows, so VoiceOver reads only the group name here.
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(providerTitle(group.provider))
            .accessibilityAddTraits(.isHeader)
            // Account order is the reader's, so a sign-in never moves a row.
            ForEach(Array(group.accounts.enumerated()), id: \.element.id) { index, account in
                Group {
                    if account.windows.isEmpty {
                        SignedOutRow(account: account, renewing: model.renewing.contains(account.id), onRenew: model.renew)
                    } else {
                        AccountRow(account: account, kinds: group.kinds, columns: columns, theme: theme, now: now,
                                   showsReading: model.sharedReading(now: now) == nil,
                                   history: model.history, readFailed: model.readError != nil, readAt: model.readAt)
                    }
                }
                .padding(.horizontal, Layout.rowPadding)
                .background(index.isMultiple(of: 2) ? Color.clear : theme.stripe)
                .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
            }
        }
    }

    private var legend: some View {
        Text("Each limit shows used % · resets in; the line below says when it fills at this pace. A dash means no reading.")
    }

    private var mascot: some View {
        model.mascot(now: now, height: Layout.openMascot, live: live)
            .frame(width: 2 * Layout.openMascot, height: Layout.openMascot)
    }

    /// Who the mascot is and who drew it; the credit is also what the guests' licences ask for.
    private var credit: some View {
        let turn = model.mascotTurn(now: now)
        return VStack(alignment: .trailing, spacing: 2) {
            Text(MascotCast.name(turn)).foregroundStyle(Palette.secondary)
            Text(MascotCast.guest(turn)?.credit ?? "swarm's own art")
        }
        .multilineTextAlignment(.trailing)
        .frame(maxWidth: 260, alignment: .trailing)
    }

    /// The best account and its pace left of the camera, the phase and read age right of it.
    private var status: some View {
        let wing = max((Layout.openWidth - 2 * Layout.inset - model.notch.width) / 2 - 12, 0)
        return HStack(spacing: 0) {
            HStack(spacing: 8) {
                if let best = bestAccount(model.accounts, history: model.history, now: now) {
                    Circle().fill(theme.fill(best.severity)).frame(width: 7, height: 7)
                    // A long name gives way; the pace always shows.
                    HStack(spacing: 0) {
                        Text("Best ").foregroundStyle(Palette.tertiary).fixedSize()
                        if best.isOld(readFailed: model.readError != nil) { Text("~").fixedSize() }
                        Text(best.account.name).lineLimit(1).truncationMode(.middle)
                        Text(" · \(providerTitle(best.account.provider)) · ").fixedSize()
                        Text(best.paceText(now: now)).fixedSize()
                            .foregroundStyle(best.severity == .calm ? Palette.primary : Palette.ink(best.severity))
                    }
                    .foregroundStyle(Palette.primary)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(best.spoken(now: now))
                } else {
                    // The cause names the fix (for example "swarm not found; set SWARM_BIN").
                    Text(model.readError.map { "\($0) · retries each minute" } ?? "No usage reading")
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(1).truncationMode(.tail)
                        .help(model.readError ?? "")
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

    private var age: String {
        if model.readError != nil { return "read failed" }
        guard let readAt = model.readAt else { return "reading…" }
        let minutes = Int(now.timeIntervalSince(readAt) / 60)
        return minutes < 1 ? "updated now" : "updated \(minutes)m ago"
    }
}

