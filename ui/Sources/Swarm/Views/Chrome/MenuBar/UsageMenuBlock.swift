import SwiftUI
import SwarmCore

/// The limits, drawn as cards inside the menu.
///
/// **A custom view in an `NSMenuItem`, and only for this block.** Everything else in the menu is an
/// ordinary row, because that is what the menu was for: the owner asked for the old menu back with
/// the new limits inside it. A proportion is the one thing a menu item's title cannot draw, so this
/// is where a view earns its place.
struct UsageMenuBlock: View {
    let model: UsageMenuModel
    let metrics: [AgentKind: [UsageMetric]]
    let accounts: [AgentKind: AgentAccount]
    let now: Date
    /// Off in the gallery, where nothing can be clicked anyway.
    var canReorder = true
    var swarmUsage = SwarmUsageBoard()

    /// The menu sizes itself to its widest item, so this decides how wide the menu is.
    static let width: CGFloat = 320

    var body: some View {
        if swarmUsage.isEmpty {
            legacyUsage
        } else {
            SwarmUsageMenuBlock(board: swarmUsage, model: model, canReorder: canReorder)
        }
    }

    private var legacyUsage: some View {
        let sections = model.layout.sections(for: metrics)
        return VStack(alignment: .leading, spacing: UsageScale.section) {
            ForEach(Array(sections.enumerated()), id: \.element.id) { index, section in
                provider(
                    section,
                    canMoveUp: canReorder && index > 0,
                    canMoveDown: canReorder && index < sections.count - 1,
                    neighbourAbove: index > 0 ? sections[index - 1].provider : nil,
                    neighbourBelow: index < sections.count - 1 ? sections[index + 1].provider : nil
                )
            }
        }
        .frame(width: Self.width, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .fixedSize()
    }

    private func provider(
        _ section: UsageLayout.Section,
        canMoveUp: Bool,
        canMoveDown: Bool,
        neighbourAbove: AgentKind?,
        neighbourBelow: AgentKind?
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 5) {
                ProviderMarkView(provider: section.provider)
                    .foregroundStyle(MenuInk.secondary)
                    .frame(width: 15, height: 15)
                Text(section.provider.label)
                    .font(UsageScale.header)
                    .foregroundStyle(MenuInk.primary)
                if let plan = accounts[section.provider]?.plan {
                    Text(plan)
                        .font(UsageScale.plan)
                        .foregroundStyle(MenuInk.secondary)
                }
                Spacer(minLength: 8)
                if canMoveUp || canMoveDown {
                    UsageProviderMoveButtons(
                        provider: section.provider,
                        above: canMoveUp ? neighbourAbove : nil,
                        below: canMoveDown ? neighbourBelow : nil,
                        model: model
                    )
                }
            }
            .padding(.leading, 2)

            VStack(spacing: 0) {
                ForEach(Array(section.visible.enumerated()), id: \.element.id) { index, metric in
                    UsageMenuRow(
                        metric: metric,
                        now: now,
                        options: model.options,
                        isCondensed: index > 0 && section.visible[index - 1].isText && metric.isText
                    )
                }
            }
            .padding(.vertical, 5)
            .background {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(MenuInk.card)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(section.provider.label)
    }

}

/// Click to move, because an open `NSMenu` cannot host a drag session.
private struct UsageProviderMoveButtons: View {
    let provider: AgentKind
    let above: AgentKind?
    let below: AgentKind?
    let model: UsageMenuModel

    var body: some View {
        HStack(spacing: 2) {
            button("chevron.up", to: above, label: "Move \(provider.label) up")
            button("chevron.down", to: below, label: "Move \(provider.label) down")
        }
    }

    @ViewBuilder
    private func button(_ symbol: String, to target: AgentKind?, label: String) -> some View {
        if let target {
            Button {
                withAnimation(.spring(response: 0.34, dampingFraction: 0.82)) {
                    model.move(provider, toward: target)
                }
            } label: {
                Image(systemName: symbol)
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(MenuInk.secondary)
                    .frame(width: 16, height: 14)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(label)
        }
    }
}

private struct SwarmUsageMenuBlock: View {
    let board: SwarmUsageBoard
    let model: UsageMenuModel
    let canReorder: Bool

    private var orderedKinds: [AgentKind] { board.providers.compactMap(\.kind) }

    var body: some View {
        VStack(alignment: .leading, spacing: UsageScale.section) {
            ForEach(board.providers, id: \.key) { provider in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 5) {
                        if let kind = provider.kind {
                            ProviderMarkView(provider: kind)
                                .foregroundStyle(MenuInk.secondary)
                                .frame(width: 15, height: 15)
                        }
                        Text(provider.title)
                            .font(UsageScale.header)
                            .foregroundStyle(MenuInk.primary)
                        Spacer(minLength: 8)
                        if canReorder, let kind = provider.kind {
                            UsageProviderMoveButtons(
                                provider: kind,
                                above: neighbour(of: kind, offset: -1),
                                below: neighbour(of: kind, offset: 1),
                                model: model
                            )
                        }
                    }
                    .padding(.leading, 2)
                    ForEach(provider.accounts, id: \.key) { account in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(account.title)
                                .font(UsageScale.label)
                                .foregroundStyle(MenuInk.primary)
                                .lineLimit(1)
                            ForEach(account.meters, id: \.self) { meter in
                                SwarmUsageMenuRow(meter: meter)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 7)
                        .background {
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .fill(MenuInk.card)
                        }
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel(provider.title)
            }
        }
        .frame(width: UsageMenuBlock.width, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .fixedSize()
    }

    private func neighbour(of provider: AgentKind, offset: Int) -> AgentKind? {
        guard let index = orderedKinds.firstIndex(of: provider) else { return nil }
        let target = index + offset
        return orderedKinds.indices.contains(target) ? orderedKinds[target] : nil
    }
}

private struct SwarmUsageMenuRow: View {
    let meter: SwarmUsageBoard.Meter

    var body: some View {
        if let message = meter.message {
            Text(message)
                .font(UsageScale.supporting)
                .foregroundStyle(MenuInk.secondary)
                .lineLimit(1)
                .accessibilityElement(children: .combine)
        } else if let window = meter.window, let usedText = meter.usedText, let fill = meter.fill {
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(window)
                        .font(UsageScale.supporting.weight(.semibold))
                        .foregroundStyle(MenuInk.primary)
                    Spacer(minLength: 8)
                    Text(usedText)
                        .foregroundStyle(MenuInk.primary)
                    if let severity = meter.severity?.word {
                        Text(severity)
                            .foregroundStyle(MenuInk.secondary)
                    }
                    if let status = meter.statusText {
                        Text(status)
                            .foregroundStyle(MenuInk.secondary)
                    }
                }
                .font(UsageScale.supporting)
                .monospacedDigit()
                .lineLimit(1)
                SwarmUsageMeterBar(fill: fill, isStale: meter.isStale, severity: meter.severity ?? .calm)
                if let reset = meter.resetText {
                    Text(reset)
                        .font(UsageScale.supporting)
                        .foregroundStyle(MenuInk.secondary)
                        .monospacedDigit()
                }
            }
            .accessibilityElement(children: .combine)
        }
    }
}

private struct SwarmUsageMeterBar: View {
    let fill: Double
    let isStale: Bool
    let severity: QuotaSeverity

    private var colour: Color {
        switch severity {
        case .calm: MenuInk.normal
        case .warning: MenuInk.warning
        case .critical, .spent: MenuInk.critical
        }
    }

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(MenuInk.track)
                if fill > 0 {
                    Capsule()
                        .fill(colour.opacity(isStale ? 0.45 : 1))
                        .frame(width: min(proxy.size.width, max(5, proxy.size.width * fill)))
                }
            }
        }
        .frame(height: 5)
        .accessibilityHidden(true)
    }
}

/// One metric: a meter for a window, a line of text for a balance.
struct UsageMenuRow: View {
    let metric: UsageMetric
    let now: Date
    let options: UsageDisplayOptions
    var isCondensed = false

    var body: some View {
        switch metric.content {
        case .meter(let quota):
            meter(UsageMeterReading.of(quota, isSession: metric.isSession, at: now, options: options))
        case .value(let text, _, _):
            value(text)
        }
    }

    private func meter(_ reading: UsageMeterReading) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(metric.title)
                    .font(UsageScale.label)
                    .foregroundStyle(MenuInk.primary)
                    .lineLimit(1)
                Spacer(minLength: 8)
                if let status = reading.status {
                    HStack(spacing: 4) {
                        if status.showsFlame {
                            Image(systemName: "flame.fill")
                                .font(UsageScale.supporting)
                                .foregroundStyle(MenuInk.critical)
                        }
                        if let text = status.text {
                            Text(text)
                                .font(UsageScale.supporting)
                                .foregroundStyle(MenuInk.secondary)
                        }
                    }
                    .lineLimit(1)
                }
            }
            UsageMeterBar(reading: reading)
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(reading.headline)
                    .foregroundStyle(MenuInk.primary)
                Spacer(minLength: 8)
                Text(reading.trailing)
                    .foregroundStyle(MenuInk.secondary)
            }
            .font(UsageScale.supporting)
            .monospacedDigit()
            .lineLimit(1)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(reading.spoken(title: metric.title))
    }

    private func value(_ text: String) -> some View {
        HStack(alignment: .center, spacing: 10) {
            Text(metric.title)
                .font(UsageScale.supporting.weight(.semibold))
                .foregroundStyle(MenuInk.primary)
            Spacer(minLength: 12)
            Text(text)
                .font(UsageScale.supporting)
                .foregroundStyle(MenuInk.primary)
                .monospacedDigit()
        }
        .lineLimit(1)
        .padding(.horizontal, 12)
        .padding(.top, isCondensed ? 2 : 6)
        .padding(.bottom, 6)
        .accessibilityElement(children: .combine)
    }
}

/// The capsule meter, with the tick where usage would be if it were spread evenly across the
/// window.
///
/// **Drawn rather than a `ProgressView`, and that was tried.** The system's linear progress bar is
/// the right control by every other measure, but it is `NSProgressIndicator` underneath, and an
/// `NSViewRepresentable` renders as SwiftUI's yellow placeholder offscreen. That would cost the
/// `limits` snapshot scene, which is the only way this block can be looked at without taking over
/// the owner's screen (see `Snapshot`). Two rounded rectangles are worth keeping for that.
struct UsageMeterBar: View {
    let reading: UsageMeterReading
    var height: CGFloat = 5

    var body: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            ZStack(alignment: .leading) {
                Capsule().fill(MenuInk.track)
                if reading.tone != .empty, reading.fill > 0 {
                    Capsule()
                        .fill(MenuInk.tone(reading.tone))
                        // Never thinner than it is tall, so the smallest honest fill is a dot.
                        .frame(width: min(width, max(height, width * reading.fill)))
                }
            }
            .overlay(alignment: .leading) {
                if let tick = reading.paceTick {
                    RoundedRectangle(cornerRadius: 1)
                        .fill(MenuInk.primary.opacity(0.55))
                        .frame(width: 2, height: height + 4)
                        .offset(x: min(max(width * tick - 1, 0), max(0, width - 2)))
                }
            }
        }
        .frame(height: height)
        .accessibilityHidden(true)
    }
}

extension UsageMetric {
    var isText: Bool {
        if case .value = content { return true }
        return false
    }
}
