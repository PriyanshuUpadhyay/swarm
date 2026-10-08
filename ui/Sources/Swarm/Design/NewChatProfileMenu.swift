import AppKit
import SwiftUI
import SwarmCore

extension View {
    func newChatProfileMenu(
        rows: [NewChatMenu.Row], refresh: @escaping () -> Void, start: @escaping (String) -> Void
    ) -> some View {
        modifier(NewChatProfileMenu(rows: rows, refresh: refresh, start: start))
    }
}

private struct NewChatProfileMenu: ViewModifier {
    let rows: [NewChatMenu.Row]
    let refresh: () -> Void
    let start: (String) -> Void

    func body(content: Content) -> some View {
        content
            .contextMenu {
                Text("New chat as…")
                if rows.isEmpty { Text("No profiles available") }
                ForEach(rows) { row in
                    Button { start(row.name) } label: {
                        Text("\(Text(name(row)))\(Text(row.caption.map { "\n" + $0 } ?? "").font(.caption))")
                    }
                }
            }
            .onHover { hovering in if hovering { refresh() } }
            .onLongPressGesture(minimumDuration: 0.4) { openMenu() }
    }

    private func name(_ row: NewChatMenu.Row) -> String {
        row.name + (row.isDefault ? " (one-click default)" : "")
    }

    /// SwiftUI has no binding that opens a context menu from a long press on macOS.
    private func openMenu() {
        let menu = NSMenu(title: "New chat as…")
        let heading = NSMenuItem(title: menu.title, action: nil, keyEquivalent: "")
        heading.isEnabled = false
        menu.addItem(heading)
        if rows.isEmpty {
            menu.addItem(withTitle: "No profiles available", action: nil, keyEquivalent: "")
        }
        let target = ProfileMenuTarget(start: start)
        for row in rows {
            let item = NSMenuItem(title: name(row), action: #selector(ProfileMenuTarget.pick(_:)), keyEquivalent: "")
            if let caption = row.caption {
                let title = NSMutableAttributedString(string: name(row), attributes: [.font: NSFont.menuFont(ofSize: 0)])
                title.append(NSAttributedString(string: "\n" + caption, attributes: [
                    .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
                    .foregroundColor: NSColor.secondaryLabelColor,
                ]))
                item.attributedTitle = title
            }
            item.target = target
            item.representedObject = row.name
            menu.addItem(item)
        }
        _ = withExtendedLifetime(target) {
            menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
        }
    }
}

@MainActor private final class ProfileMenuTarget: NSObject {
    private let start: (String) -> Void

    init(start: @escaping (String) -> Void) { self.start = start }

    @objc func pick(_ item: NSMenuItem) {
        guard let profile = item.representedObject as? String else { return }
        start(profile)
    }
}
