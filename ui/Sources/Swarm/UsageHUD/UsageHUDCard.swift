import SwiftUI
import SwarmCore

private typealias HUD = DesignTokens.UsageHUD

/// A calm bar carries the provider's color. A warning or a danger takes its place.
private func providerColors(_ provider: String) -> (base: Color, light: Color) {
    provider == "codex" ? (HUD.codex, HUD.codexLight) : (HUD.claude, HUD.claudeLight)
}

private func barColors(_ severity: UsageSeverity, provider: String) -> (base: Color, light: Color) {
    switch severity {
    case .danger: (HUD.danger, HUD.dangerLight)
    case .warn: (HUD.warn, HUD.warnLight)
    case .calm: providerColors(provider)
    }
}

private extension UsageMood {
    var dotColor: Color {
        switch self {
        case .calm: HUD.calm
        case .warn: HUD.warn
        case .danger: HUD.danger
        case .asleep: HUD.textTertiary
        case .thinking: HUD.codex
        }
    }
}

/// One line that drops to its short form, then truncates, and never wraps to a second line.
private struct FittedLine: View {
    let text: String
    let short: String

    var body: some View {
        ViewThatFits(in: .horizontal) {
            Text(verbatim: text)
            Text(verbatim: short)
        }
        .lineLimit(1)
    }
}

private struct HUDButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(HUD.button)
            .frame(width: HUD.controlSize, height: HUD.controlSize)
            .background(configuration.isPressed ? HUD.buttonPressedFill : HUD.buttonFill, in: Circle())
            .contentShape(Circle())
            .scaleEffect(configuration.isPressed ? HUD.pressedScale : 1)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }
}

/// A text button inside the dark card.
private struct HUDLinkStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(HUD.risk)
            .underline()
            .foregroundStyle(configuration.isPressed ? HUD.textSecondary : HUD.textPrimary)
    }
}

private struct UsageBar: View {
    let usedPct: Double
    let colors: (base: Color, light: Color)
    let isOlder: Bool
    let reduceMotion: Bool
    let rowIndex: Int

    @State private var grown = false

    var body: some View {
        GeometryReader { proxy in
            let target = proxy.size.width * min(max(usedPct, 0), 100) / 100
            ZStack(alignment: .leading) {
                Capsule().fill(HUD.barTrack)
                Capsule()
                    .fill(LinearGradient(colors: [colors.base, colors.light], startPoint: .leading, endPoint: .trailing))
                    .opacity(isOlder ? HUD.olderBarOpacity : 1)
                    .frame(width: grown ? target : 0)
                    // The first fill is animated by the `grown` transaction below. Later value
                    // changes come through here.
                    .animation(reduceMotion ? nil : .spring(response: 0.5, dampingFraction: 1), value: target)
            }
        }
        .frame(height: HUD.barHeight)
        .accessibilityHidden(true)
        .onAppear {
            guard !reduceMotion else {
                grown = true
                return
            }
            // The bars fill in a short cascade down the account list.
            withAnimation(.spring(response: 0.6, dampingFraction: 1).delay(0.03 * Double(rowIndex))) {
                grown = true
            }
        }
    }
}

private struct MeterCell: View {
    let meter: SwarmUsageMeter?
    let title: String
    let pressure: UsageRowPressure?
    let readAtSeconds: Int?
    let nowSeconds: Int
    let reduceMotion: Bool
    let rowIndex: Int
    /// Too narrow for the risk beside the number. The risk then takes the last line, and the reset
    /// time stays in the tooltip.
    let compact: Bool

    var body: some View {
        let detail = meter.map {
            UsageHUDModel.cellDetail(meter: $0, pressure: pressure, readAtSeconds: readAtSeconds, nowSeconds: nowSeconds)
        } ?? []
        let risk = detail.first(where: \.risk)
        let plain = detail.first { !$0.risk }
        let lastLine = compact ? risk ?? plain : plain
        let isFresh = meter?.state == .fresh
        // An older sample shows no risk, so it keeps the provider's color.
        let severity = isFresh ? UsageSeverity(usedPct: meter?.usedPct ?? 0, level: pressure?.level) : .calm
        let tint = severity.hudColor
        // A short form can hide the gap, so the tooltip always has the full lines.
        let fullLines = detail.map(\.text)
            + [meter.flatMap { UsageHUDModel.freshnessDetail($0, nowSeconds: nowSeconds) }].compactMap { $0 }
        VStack(alignment: .leading, spacing: HUD.lineSpacing) {
            if let meter, let usedPct = meter.usedPct {
                HStack(alignment: .firstTextBaseline, spacing: HUD.valueSpacing) {
                    Text(verbatim: "\(Int(usedPct.rounded()))%")
                        .font(HUD.value)
                        .monospacedDigit()
                        .contentTransition(.numericText())
                        .animation(reduceMotion ? nil : .spring(response: 0.4, dampingFraction: 1), value: usedPct)
                        .foregroundStyle(isFresh ? tint ?? HUD.textPrimary : HUD.textOlder)
                        .fixedSize()
                    if meter.state == .stale {
                        Image(systemName: "clock")
                            .font(HUD.olderMark)
                            .foregroundStyle(HUD.textTertiary)
                            .accessibilityLabel("Older sample")
                    }
                    // The risk sits on the number's line, so a cell at risk is no taller than a calm one.
                    if let risk, !compact {
                        Spacer(minLength: HUD.lineSpacing)
                        FittedLine(text: risk.text, short: risk.short)
                            .font(HUD.risk)
                            .foregroundStyle(tint ?? HUD.textSecondary)
                    }
                }
                UsageBar(
                    usedPct: usedPct,
                    colors: barColors(severity, provider: meter.provider),
                    isOlder: !isFresh, reduceMotion: reduceMotion, rowIndex: rowIndex
                )
                FittedLine(text: lastLine?.text ?? "", short: lastLine?.short ?? "")
                    .font(lastLine?.risk == true ? HUD.risk : HUD.detail)
                    .foregroundStyle(lastLine?.risk == true ? tint ?? HUD.textSecondary : HUD.textSecondary)
            } else {
                Text(verbatim: "—").font(HUD.noValue).foregroundStyle(HUD.textSecondary)
                Text("No sample").font(HUD.detail).foregroundStyle(HUD.textSecondary)
            }
        }
        .frame(maxWidth: .infinity, minHeight: HUD.rowHeight, maxHeight: HUD.rowHeight, alignment: .topLeading)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(meter?.usedPct.map { "\(Int($0.rounded())) percent used" } ?? "No sample")")
        .accessibilityValue(fullLines.joined(separator: ". "))
        .help(fullLines.isEmpty ? title : fullLines.joined(separator: "\n"))
    }
}

private struct ProviderCard: View {
    @Environment(\.colorSchemeContrast) private var contrast
    let section: UsageHUDSection
    let pressures: [UsageRowPressure]
    let readAtSeconds: Int?
    let nowSeconds: Int
    let reduceMotion: Bool
    /// The index of this card's first account in the whole list, so the bar cascade runs down
    /// all cards and does not start again per provider.
    let rowIndexOffset: Int
    /// The whole card with its padding.
    let width: CGFloat
    let openAccounts: () -> Void

    @State private var hoveredAccount: String?

    private var innerWidth: CGFloat { width - 2 * HUD.cardPaddingHorizontal }
    /// Names get a little over a quarter of the card. Meters take the rest.
    private var accountWidth: CGFloat {
        min(HUD.accountMaxWidth, max(HUD.accountMinWidth, innerWidth * HUD.accountWidthShare))
    }
    private var compactCells: Bool {
        let columns = CGFloat(max(1, section.windows.count))
        return (innerWidth - accountWidth - HUD.columnSpacing * columns) / columns < HUD.wideCell
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: HUD.columnSpacing) {
                HStack(spacing: HUD.labelSpacing) {
                    Circle().fill(providerColors(section.provider).base)
                        .frame(width: HUD.providerDot, height: HUD.providerDot)
                        .accessibilityHidden(true)
                    Text(verbatim: section.title).font(HUD.provider).foregroundStyle(HUD.textPrimary)
                }
                .frame(width: accountWidth, alignment: .leading)
                ForEach(section.windows) { window in
                    Text(verbatim: window.title.uppercased())
                        .font(HUD.columnTitle)
                        .tracking(HUD.columnTitleTracking)
                        .foregroundStyle(HUD.textSecondary)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if section.windows.isEmpty { Spacer(minLength: 0) }
            }
            .padding(.bottom, HUD.rowPadding)

            ForEach(Array(section.accounts.enumerated()), id: \.element.id) { index, account in
                accountRow(account, index: index)
            }
        }
        .padding(.horizontal, HUD.cardPaddingHorizontal)
        .padding(.vertical, HUD.cardPaddingVertical)
        .frame(width: width)
        .background(RoundedRectangle(cornerRadius: HUD.cardRadius).fill(HUD.cardInset))
        .overlay(RoundedRectangle(cornerRadius: HUD.cardRadius).stroke(HUD.hairline, lineWidth: DesignTokens.Size.hairline))
    }

    private func accountRow(_ account: UsageHUDAccount, index: Int) -> some View {
        let subtitle = UsageHUDModel.accountSubtitle(account.meters, nowSeconds: nowSeconds)
        return HStack(spacing: HUD.columnSpacing) {
            VStack(alignment: .leading, spacing: HUD.lineSpacing) {
                Text(verbatim: account.name)
                    .font(HUD.account)
                    .foregroundStyle(HUD.textPrimary)
                    .lineLimit(1).truncationMode(.middle)
                    .help(account.name)
                Text(verbatim: subtitle.text)
                    .font(HUD.detail)
                    .foregroundStyle(subtitle.warning ? HUD.warn : HUD.textSecondary)
                    .lineLimit(1)
                    .help(subtitle.warning
                          ? "Refresh could not update this account. The value shown is the last sample."
                          : "When this account's usage was last confirmed")
            }
            .frame(width: accountWidth, alignment: .leading)
            if let notice = account.notice {
                HStack(spacing: HUD.headerSpacing) {
                    Text(verbatim: notice.text)
                        .font(HUD.caption).foregroundStyle(HUD.textSecondary)
                        .lineLimit(2)
                        .help(notice.text)
                    if notice.needsSignIn {
                        Button("Open Accounts", action: openAccounts).buttonStyle(HUDLinkStyle())
                    }
                }
                .frame(maxWidth: .infinity, minHeight: HUD.rowHeight, maxHeight: HUD.rowHeight, alignment: .leading)
            } else {
                ForEach(section.windows) { window in
                    let meter = account.meters.first { $0.window == window.id }
                    MeterCell(
                        meter: meter, title: window.title,
                        pressure: meter.flatMap { meter in pressures.first { $0.meterID == meter.id } },
                        readAtSeconds: readAtSeconds, nowSeconds: nowSeconds, reduceMotion: reduceMotion,
                        rowIndex: rowIndexOffset + index, compact: compactCells
                    )
                }
            }
        }
        .padding(.vertical, HUD.rowPadding)
        .background(
            RoundedRectangle(cornerRadius: HUD.rowRadius)
                .fill(hoveredAccount == account.id ? HUD.rowHover : .clear)
        )
        .onHover { inside in
            withAnimation(.easeOut(duration: 0.12)) {
                hoveredAccount = inside ? account.id : (hoveredAccount == account.id ? nil : hoveredAccount)
            }
        }
        .overlay(alignment: .top) {
            if index > 0 {
                Rectangle()
                    .fill(contrast == .increased ? HUD.strongHairline : HUD.hairline)
                    .frame(height: HUD.hairlineWidth)
                    .padding(.horizontal, HUD.rowDividerInset)
            }
        }
    }
}

/// The open HUD: a header with the mood, one card per provider, and a footer.
struct UsageHUDCard: View {
    let model: UsageHUDModel
    let actions: UsageHUDActions
    let reduceMotion: Bool
    @State private var naturalHeight: CGFloat = 0
    @State private var refreshBounce = 0

    private var contentWidth: CGFloat { model.panelWidth - 2 * HUD.contentInset }
    private var chromeHeight: CGFloat { HUD.headerHeight + HUD.footerHeight }
    /// The screen is the ceiling. A height that the owner set with the bottom edge can lower it.
    private var availableHeight: CGFloat {
        min(model.panelHeightLimit ?? .infinity, max(0, model.panelSize.height - model.notchTopInset))
    }
    private var visibleHeight: CGFloat {
        min(naturalHeight > 0 ? naturalHeight + chromeHeight : availableHeight, availableHeight)
    }

    var body: some View {
        // The ages and the run-out lines move with the clock, one time each minute.
        TimelineView(.periodic(from: .now, by: 60)) { context in
            let nowSeconds = Int(context.date.timeIntervalSince1970)
            VStack(spacing: 0) {
                header.frame(height: HUD.headerHeight)
                // Only the account list scrolls, so the controls stay in view.
                ScrollView(.vertical) {
                    accountList(nowSeconds: nowSeconds)
                        .padding(.vertical, HUD.rowPadding)
                        .frame(width: contentWidth)
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height in
                            naturalHeight = height
                            model.expandedContentHeight = min(height + chromeHeight, availableHeight)
                        }
                }
                .scrollBounceBehavior(.basedOnSize)
                .frame(height: max(0, visibleHeight - chromeHeight))
                footer(nowSeconds: nowSeconds).frame(height: HUD.footerHeight)
            }
        }
        .padding(.horizontal, HUD.contentInset)
        .frame(width: model.panelWidth, height: visibleHeight, alignment: .top)
    }

    @ViewBuilder
    private func accountList(nowSeconds: Int) -> some View {
        let sections = model.sections
        if model.firstReadFailed {
            emptyState(symbol: "exclamationmark.circle", title: "Usage is unavailable",
                       detail: "Swarm could not read usage. Click Refresh to try again.")
        } else if !model.hasLoaded {
            HStack(spacing: HUD.headerSpacing) {
                ProgressView().controlSize(.small)
                Text("Reading usage…").font(HUD.body).foregroundStyle(HUD.textSecondary)
            }
            .frame(maxWidth: .infinity, minHeight: HUD.emptyHeight)
        } else if sections.isEmpty {
            VStack(spacing: HUD.headerSpacing) {
                emptyState(symbol: "person.crop.circle.badge.plus", title: "No accounts yet",
                           detail: "Add a Claude or Codex account in Settings › Accounts.")
                Button("Open Accounts", action: actions.openAccounts).buttonStyle(HUDLinkStyle())
            }
        } else {
            let widths = UsageHUDLayout.providerCardWidths(
                minWidths: sections.map { UsageHUDLayout.providerCardMinWidth(columns: $0.windows.count) },
                total: contentWidth, gap: HUD.cardGap
            )
            // One layout value, so the cards move when the width passes the limit and are not rebuilt.
            let layout = widths == nil
                ? AnyLayout(VStackLayout(alignment: .leading, spacing: HUD.cardGap))
                : AnyLayout(HStackLayout(alignment: .top, spacing: HUD.cardGap))
            layout {
                ForEach(Array(sections.enumerated()), id: \.element.id) { index, section in
                    ProviderCard(
                        section: section, pressures: model.pressures, readAtSeconds: model.readAtSeconds,
                        nowSeconds: nowSeconds, reduceMotion: reduceMotion,
                        rowIndexOffset: sections[..<index].reduce(0) { $0 + $1.accounts.count },
                        width: widths?[index] ?? contentWidth, openAccounts: actions.openAccounts
                    )
                }
            }
        }
    }

    private var header: some View {
        HStack(spacing: HUD.headerSpacing) {
            VStack(alignment: .leading, spacing: HUD.lineSpacing) {
                Text("Usage").font(HUD.title).foregroundStyle(HUD.textPrimary)
                HStack(spacing: HUD.valueSpacing) {
                    Circle().fill(model.mood.dotColor).frame(width: HUD.moodDot, height: HUD.moodDot)
                        .accessibilityHidden(true)
                    Text(verbatim: model.headerCaption).font(HUD.caption).foregroundStyle(HUD.textSecondary)
                }
            }
            Spacer(minLength: HUD.lineSpacing)
            Button {
                refreshBounce += 1
                actions.refresh()
            } label: {
                // The glyph turns in place, so the header does not move during a refresh.
                Image(systemName: "arrow.clockwise")
                    .symbolEffect(.bounce, value: refreshBounce)
                    .rotationEffect(.degrees(model.isRefreshing && !reduceMotion ? 360 : 0))
                    .animation(model.isRefreshing && !reduceMotion
                               ? .linear(duration: 0.9).repeatForever(autoreverses: false)
                               : .easeOut(duration: 0.2),
                               value: model.isRefreshing)
            }
            .buttonStyle(HUDButtonStyle())
            .disabled(model.isRefreshing)
            .help("Refresh usage (⌘R)")
            .accessibilityLabel("Refresh usage")
            .keyboardShortcut("r", modifiers: .command)
            Button(action: actions.close) {
                Image(systemName: "xmark")
            }
            .buttonStyle(HUDButtonStyle())
            .help("Close usage (Esc)")
            .accessibilityLabel("Close usage")
            .keyboardShortcut(.cancelAction)
        }
        .foregroundStyle(HUD.textPrimary)
    }

    private func footer(nowSeconds: Int) -> some View {
        let footer = model.footer(nowSeconds: nowSeconds)
        return HStack(spacing: HUD.rowDividerInset) {
            // An icon only where it explains something: a warning, or the clock of an older sample.
            if footer.warning {
                Image(systemName: "exclamationmark.circle").font(HUD.caption)
            } else if model.hasOlderSamples {
                Image(systemName: "clock").font(HUD.caption)
            }
            Text(verbatim: footer.text).font(HUD.footer).lineLimit(1)
            Spacer(minLength: 0)
        }
        .foregroundStyle(footer.warning ? HUD.warn : HUD.textSecondary)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .top) { HUD.hairline.frame(height: HUD.hairlineWidth).offset(y: -HUD.rowPadding) }
    }

    private func emptyState(symbol: String, title: String, detail: String) -> some View {
        VStack(spacing: HUD.headerSpacing) {
            Image(systemName: symbol).font(HUD.emptySymbol).foregroundStyle(HUD.textSecondary)
            Text(verbatim: title).font(HUD.emptyTitle).foregroundStyle(HUD.textPrimary)
            Text(verbatim: detail).font(HUD.body).foregroundStyle(HUD.textSecondary)
                .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, HUD.emptyInset)
        .frame(maxWidth: .infinity, minHeight: HUD.emptyHeight)
    }
}
