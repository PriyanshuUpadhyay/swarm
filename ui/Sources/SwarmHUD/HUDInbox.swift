// ADR 0147: the popup under the notch for what just came in, and the open card's Inbox section:
// the agents that wait on the owner, with their choices, and the chats that ended since the owner
// looked.
import SwarmHUDCore
import SwiftUI

enum InboxLayout {
    static let popupWidth: CGFloat = 460
    /// A popup line: the title and kind word, then up to two lines of the reply.
    static let popupItem: CGFloat = 58
    static let popupBottom: CGFloat = 10
    /// The popup's top row under the camera, when the wings have no notch to sit beside.
    static let popupGap: CGFloat = 4
    static let sectionHeader: CGFloat = 26
    /// A need card: its title, the question, and a row of choice buttons.
    static let needCard: CGFloat = 92
    static let row: CGFloat = 36
    /// The open card shows this many need cards and rows; the rest are a "+N more".
    static let shownNeeds = 2
    static let shownRows = 5
    static let below: CGFloat = 6

    static func height(_ inbox: InboxSnapshot, error: Bool) -> CGFloat {
        var height: CGFloat = 0
        if !inbox.needs.isEmpty { height += sectionHeader + CGFloat(min(inbox.needs.count, shownNeeds)) * needCard }
        if !inbox.rows.isEmpty { height += sectionHeader + CGFloat(min(inbox.rows.count, shownRows)) * row }
        if error { height += row }
        return height == 0 ? 0 : height + below
    }
}

extension UsageModel {
    var popupSize: CGSize {
        let items = CGFloat(popup?.items.count ?? 0)
        return CGSize(width: max(closedSize.width, InboxLayout.popupWidth),
                      height: notch.height + InboxLayout.popupGap + items * InboxLayout.popupItem + InboxLayout.popupBottom)
    }
}

/// Amber for a wait or a question, red for a stop, green for done.
private func kindColor(_ kind: InboxKind) -> Color {
    switch kind {
    case .failed: return Palette.critical
    case .done: return Palette.resets
    case .asks, .waiting: return Palette.warn
    }
}

/// The wings stay on top, so the popup grows out of the notch the owner already knows.
struct PopupContent: View {
    let model: UsageModel
    let theme: Theme
    let live: Bool
    let now: Date

    var body: some View {
        VStack(spacing: 0) {
            ClosedWings(model: model, theme: theme, live: live, now: now)
                .frame(width: model.closedSize.width, height: model.notch.height)
            Spacer().frame(height: InboxLayout.popupGap)
            ForEach(model.popup?.items ?? [], id: \.key) { item in
                PopupLine(item: item, error: model.popupError?.key == item.key ? model.popupError?.text : nil,
                          open: { model.openChat(session: item.session, agent: item.agent, line: item.key) },
                          dismiss: { model.popup = nil })
                    .frame(height: InboxLayout.popupItem)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Layout.ear)
        .frame(width: model.popupSize.width)
    }
}

struct PopupLine: View {
    let item: PopupItem
    /// Why Open failed; it takes the reply's place for a few seconds.
    let error: String?
    let open: () -> Void
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Circle().fill(kindColor(item.kind)).frame(width: 7, height: 7)
                    Text(item.title).fontWeight(.semibold).foregroundStyle(Palette.primary)
                        .lineLimit(1).truncationMode(.tail)
                    Text(item.word).foregroundStyle(kindColor(item.kind)).fixedSize()
                }
                .font(.system(size: 12))
                Text(error ?? item.body).font(.system(size: 11)).foregroundStyle(error == nil ? Palette.secondary : Palette.warn)
                    .lineLimit(2).truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Button("Open", action: open).buttonStyle(InboxButton(prominent: true))
            Button(action: dismiss) {
                Image(systemName: "xmark").font(.system(size: 9, weight: .bold)).frame(width: 18, height: 18)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain).foregroundStyle(Palette.tertiary)
            .help("Hide; the chat stays in the inbox").accessibilityLabel("Hide")
        }
        .padding(.horizontal, Layout.rowPadding)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(item.title), \(item.word): \(item.body)")
    }
}

/// The open card's top section; empty when nothing waits and the owner has seen every result.
struct InboxSection: View {
    let model: UsageModel
    let theme: Theme

    var body: some View {
        let inbox = model.inbox
        VStack(spacing: 0) {
            if !inbox.needs.isEmpty {
                header("Needs you", more: inbox.needsTotal - min(inbox.needs.count, InboxLayout.shownNeeds))
                ForEach(inbox.needs.prefix(InboxLayout.shownNeeds), id: \.key) { need in
                    NeedCardView(need: need, answering: model.answering.contains(need.key),
                                 answer: { model.answer(need, choice: $0) },
                                 open: { model.openChat(session: need.session, agent: need.agent) })
                        .frame(height: InboxLayout.needCard)
                }
            }

            if !inbox.rows.isEmpty {
                HStack {
                    header("Since you looked", more: inbox.rowsTotal - min(inbox.rows.count, InboxLayout.shownRows))
                    Button("Clear") { model.clearInbox() }.buttonStyle(InboxButton(prominent: false))
                        .help("Mark these chats seen")
                }
                ForEach(inbox.rows.prefix(InboxLayout.shownRows), id: \.id) { row in
                    rowView(row).frame(height: InboxLayout.row)
                }
            }

            if let error = model.shownError {
                Text(error).font(.system(size: 11)).foregroundStyle(Palette.warn)
                    .lineLimit(1).truncationMode(.tail)
                    .frame(maxWidth: .infinity, minHeight: InboxLayout.row, alignment: .leading)
                    .padding(.horizontal, Layout.rowPadding)
            }
        }
    }

    private func header(_ title: String, more: Int) -> some View {
        HStack(spacing: 6) {
            Text(title).foregroundStyle(Palette.tertiary)
            if more > 0 { Text("+\(more) more").foregroundStyle(Palette.tertiary) }
            Spacer(minLength: 0)
        }
        .font(.system(size: 11, weight: .medium))
        .padding(.horizontal, Layout.rowPadding)
        .frame(height: InboxLayout.sectionHeader)
        .accessibilityAddTraits(.isHeader)
    }

    /// The whole row opens the chat.
    private func rowView(_ row: InboxRow) -> some View {
        Button { model.openChat(session: row.session, agent: row.agent) } label: {
            HStack(spacing: 8) {
                Circle().fill(kindColor(row.kind)).frame(width: Layout.dot, height: Layout.dot)
                Text(row.title).fontWeight(.semibold).foregroundStyle(Palette.primary).lineLimit(1)
                Text(row.kind.word).foregroundStyle(kindColor(row.kind)).fixedSize()
                Text(row.body).foregroundStyle(Palette.secondary).lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 0)
                Text("Open").foregroundStyle(Palette.tertiary).fixedSize()
            }
            .font(.system(size: 12))
            .padding(.horizontal, Layout.rowPadding)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .overlay(alignment: .top) { Palette.rule.frame(height: 1) }
        .accessibilityLabel("\(row.title), \(row.kind.word): \(row.body)")
        .accessibilityHint("Opens the chat")
    }
}

/// A waiting agent: its chat, the question on its screen, and a button per choice.
struct NeedCardView: View {
    let need: NeedCard
    let answering: Bool
    let answer: (Int) -> Void
    let open: () -> Void

    /// More choices than this send the owner to the chat, where every choice fits.
    private let shownChoices = 4

    var body: some View {
        Group {
            if let prompt = need.prompt, prompt.kind == "plan" {
                let pointer = "Open to read the full plan and answer."
                if prompt.choices.isEmpty {
                    card(pointer, choices: [])
                } else {
                    let plan = prompt.question.split(separator: "\n", maxSplits: 1).last?
                        .trimmingCharacters(in: .whitespacesAndNewlines) ?? prompt.question
                    ViewThatFits(in: .vertical) {
                        card(plan, choices: prompt.choices, fullPlan: true)
                            .fixedSize(horizontal: false, vertical: true)
                        card(pointer, choices: [])
                    }
                }
            } else {
                let choices = need.prompt?.choices ?? []
                card(need.prompt?.question ?? "Waiting on a permission; its question could not be read.",
                     choices: choices.count <= shownChoices ? choices : [])
            }
        }
        .padding(.horizontal, Layout.rowPadding).padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 10).fill(Palette.chipBlocked))
        .padding(.vertical, 2)
        .accessibilityElement(children: .contain)
    }

    private func card(_ question: String, choices: [String], fullPlan: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Circle().fill(Palette.warn).frame(width: 7, height: 7)
                Text(need.title).fontWeight(.semibold).foregroundStyle(Palette.primary).lineLimit(1)
                Text(need.agent).foregroundStyle(Palette.tertiary).lineLimit(1)
                Spacer(minLength: 0)
            }
            .font(.system(size: 12))
            Text(question)
                .font(.system(size: 11)).foregroundStyle(Palette.secondary)
                .lineLimit(fullPlan ? nil : 2).truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 6) {
                ForEach(Array(choices.enumerated()), id: \.offset) { index, choice in
                    Button(choice) { answer(index) }.buttonStyle(InboxButton(prominent: index == 0))
                        .disabled(answering)
                }
                Spacer(minLength: 0)
                Button("Open", action: open).buttonStyle(InboxButton(prominent: false))
            }
        }
    }
}

struct InboxButton: ButtonStyle {
    let prominent: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 11, weight: .semibold))
            .lineLimit(1).truncationMode(.tail)
            .foregroundStyle(prominent ? Color.black : Palette.primary)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .background(Capsule().fill(prominent ? Palette.primary : Color.white.opacity(0.12)))
            .opacity(configuration.isPressed ? 0.7 : 1)
            .contentShape(Capsule())
    }
}
