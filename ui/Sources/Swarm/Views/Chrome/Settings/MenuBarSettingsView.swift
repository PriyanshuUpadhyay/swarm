import SwiftUI
import SwarmCore

/// Settings ▸ Menu Bar: what the menu bar item shows, and what holds the Mac awake.
///
/// **The usage half of this pane is gone**, with the figures, the meter styles and the per-provider
/// star picker it configured. Swarm no longer reports usage, because Jellow already does. What is
/// left is the item itself, the two counts beside the mark, and Keep Awake.
struct MenuBarSettingsView: View {
    let app: AppModel

    @AppStorage(MenuBarStatusItem.settingKey) private var showsItem = MenuBarStatusItem.isOnByDefault
    @State private var model = MenuBarPreferences.shared
    @State private var keepAwake = KeepAwakeModel.shared
    @State private var sleepSwitch = SleepSwitch.shared

    var body: some View {
        Form {
            Section {
                preview
                    .frame(maxWidth: .infinity)
                    .listRowInsets(EdgeInsets(top: 10, leading: 10, bottom: 10, trailing: 10))
            }

            Section("The menu bar item") {
                // Never disabled, whatever is off below it: this switch is the way back. It was
                // inside the `disabled` that covers the rest, which left somebody who turned the
                // item off with a greyed out switch and no way to return.
                Toggle("Show Swarm in the menu bar", isOn: $showsItem)
                Group {
                    Toggle(isOn: $model.showsWaitingCount) {
                        Label("Count agents waiting on you", systemImage: MenuBarSummary.waitingSymbol)
                    }
                    Toggle(isOn: $model.showsUnreadCount) {
                        Label("Count finished tasks", systemImage: MenuBarSummary.unreadSymbol)
                    }
                }
                .disabled(!showsItem)
            }

            Section("Keep Awake") {
                Toggle("Show a cup while kept awake", isOn: $model.showsCup)
                    .disabled(!showsItem)
                Toggle(isOn: $keepAwake.keepsLidClosed) {
                    Text("Keep awake with the lid closed")
                    Text("Needs Swarm's helper, approved once in System Settings.")
                }
                switch sleepSwitch.standing {
                case .ready:
                    Label(
                        "Swarm's helper is approved. Sleep is restored when the session ends, or if Swarm crashes.",
                        systemImage: "checkmark.circle.fill"
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                case .needsApproval:
                    HStack {
                        Text("Allow Swarm's helper to finish switching this on.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Button("Open System Settings") { sleepSwitch.openApprovalSettings() }
                    }
                case .unavailable(let reason):
                    Text("This build cannot install the helper: \(reason)")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .onAppear { sleepSwitch.refresh() }
    }

    // MARK: - The live preview

    /// The item itself, so this pane cannot disagree with the thing it is configuring.
    private var preview: some View {
        VStack(spacing: 8) {
            HStack(spacing: 10) {
                if showsItem {
                    Image(systemName: "point.3.connected.trianglepath.dotted")
                        .foregroundStyle(.white)
                    if model.showsCup {
                        Image(systemName: KeepAwake.menuBarSymbol)
                            .foregroundStyle(.white.opacity(keepAwake.isActive ? 1 : 0.35))
                    }
                    // Faded at zero, like the cup when nothing is keeping the Mac awake, so the
                    // switches below visibly do something on a quiet afternoon. The real item
                    // leaves a zero out.
                    if model.showsWaitingCount {
                        previewCount(MenuBarSummary.waitingSymbol, count: app.waitingCount)
                    }
                    if model.showsUnreadCount {
                        previewCount(MenuBarSummary.unreadSymbol, count: unreadCount)
                    }
                } else {
                    Text("No menu bar item")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.5))
                }
            }
            .id(previewIdentity)
            .transition(.opacity)
            .padding(.horizontal, 14)
            .frame(height: 28)
            .frame(minWidth: 120)
            .background(Color.black.opacity(0.85), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .animation(.spring(response: 0.32, dampingFraction: 0.86), value: previewIdentity)

            Text("What the menu bar shows right now")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 6)
    }

    /// Everything the drawn item depends on, in one value, so the preview knows when it changed.
    private var previewIdentity: String {
        [
            showsItem ? "on" : "off",
            model.showsCup ? "cup" : "nocup",
            keepAwake.isActive ? "awake" : "asleep",
            model.showsWaitingCount ? "waiting \(app.waitingCount)" : "nowaiting",
            model.showsUnreadCount ? "unread \(unreadCount)" : "nounread",
        ].joined(separator: "|")
    }

    /// The Dock badge's figure, which is the one the status item is handed.
    private var unreadCount: Int {
        DockBadge.unreadCount(in: app.workspaces, isRunning: app.isRunning)
    }

    private func previewCount(_ symbol: String, count: Int) -> some View {
        HStack(spacing: 3) {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
            Text(String(count))
                .monospacedDigit()
        }
        .foregroundStyle(.white.opacity(count > 0 ? 1 : 0.35))
    }
}
