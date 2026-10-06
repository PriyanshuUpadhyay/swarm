import SwiftUI
import SwarmCore

/// Swarm > Managed Changes…: each item swarm wrote outside its home, one row per writer and file
/// (ADR 0042). A row's switch is its undo: off shows the removal's diff in the hooks setup sheet
/// before anything is written, and on runs the writer's own plan again. The page holds no write
/// logic; each write is the CLI behind that sheet (ADR 0036).
struct ManagedChangesPage: View {
    @State private var list: SwarmManagedList?
    /// `list`'s groups and summary, made once per load, not once per body pass.
    @State private var groups: [SwarmManagedList.Group] = []
    @State private var summary = ""
    @State private var error: String?
    @State private var isLoading = true
    @State private var sheet: Sheet?
    /// The newest load; an older load that answers late writes nothing.
    @State private var loads = 0
    /// A hooks item the owner removed is not offered again at launch.
    @AppStorage("hooksSetupDeclined") private var hooksSetupDeclined = false
    /// Closed group names, joined by newlines; a group is open by default.
    @AppStorage("managed.collapsedGroups") private var collapsedGroups = ""
    @Environment(\.controlActiveState) private var activeState
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let bus = SwarmCLIBus()

    private enum Sheet: Identifiable {
        case undo(ids: [String], hooks: Bool)
        case setUp

        var id: String {
            switch self {
            case .undo(let ids, _): ids.joined(separator: ",")
            case .setUp: "setUp"
            }
        }
    }

    var body: some View {
        let collapsed = self.collapsed
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            header
            if let error {
                HStack {
                    Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
                    Button("Retry") { Task { await load() } }
                }
            }
            if !groups.isEmpty {
                ScrollView {
                    // Each header and row is its own child, so a long group builds only the rows
                    // on screen.
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(groups) { group in
                            let isOpen = !collapsed.contains(group.name)
                            groupHeader(group, isOpen: isOpen)
                            if isOpen {
                                ForEach(group.rows) { row in managedRow(row, in: group) }
                            }
                        }
                    }
                }
            } else if isLoading {
                DelayedProgress("Loading managed changes")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if error == nil {
                ContentUnavailableView(
                    "Swarm has changed nothing outside its home",
                    systemImage: "checkmark.shield"
                )
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(minWidth: DesignTokens.Size.hooksSheet, maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task { await load() }
        .onChange(of: activeState) { _, state in
            if state == .key { Task { await load() } }
        }
        .sheet(item: $sheet, onDismiss: { Task { await load() } }) { kind in
            switch kind {
            case .undo(let ids, let hooks):
                HooksSetupSheet(
                    loadPlan: { try await bus.managedRevertPlan(ids: ids) },
                    setUp: { digest in
                        try await bus.revertManaged(ids: ids, digest: digest)
                        if hooks { hooksSetupDeclined = true }
                    },
                    notNow: { sheet = nil },
                    done: { sheet = nil },
                    copy: .undo
                )
            case .setUp:
                HooksSetupSheet(
                    loadPlan: { try await bus.hooksPlan() },
                    setUp: { try await bus.setUpHooks(digest: $0) },
                    notNow: { sheet = nil },
                    done: { sheet = nil }
                )
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            HStack {
                Text("Managed Changes").font(.largeTitle.bold())
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                if isLoading && list != nil { DelayedProgress() }
                Button { Task { await load() } } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .disabled(isLoading)
                .help("Read the files again")
                .accessibilityLabel("Refresh")
            }
            Text("Swarm adds only its own entries and removes only what it wrote, after you see the diff.")
                .foregroundStyle(.secondary)
            if let list, !groups.isEmpty {
                HStack {
                    Text(summary).font(.callout)
                    Spacer()
                    Button("Undo All…") { undo(list.presentIDs, in: list.entries) }
                        .disabled(list.presentIDs.isEmpty)
                }
            }
        }
    }

    /// A group's name that collapses it, as the sidebar's project header does.
    private func groupHeader(_ group: SwarmManagedList.Group, isOpen: Bool) -> some View {
        HStack {
            Button {
                withAnimation(reduceMotion ? nil : DesignTokens.spring) { toggle(group.name) }
            } label: {
                HStack(spacing: DesignTokens.Spacing.xs) {
                    Image(systemName: "chevron.right")
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                        .frame(width: DesignTokens.Size.glyphSlot)
                    Text(group.name).font(.subheadline.weight(.semibold))
                    if let source = group.source {
                        Text("· \(source)").font(.caption).foregroundStyle(.tertiary)
                    }
                }
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: DesignTokens.Size.row, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(group.spoken)
            .accessibilityAddTraits(.isHeader)
            .accessibilityValue(isOpen ? "expanded" : "collapsed")
            if !group.presentIDs.isEmpty {
                Button("Undo \(group.name)…") { undo(group.presentIDs, in: list?.entries ?? []) }
                    .buttonStyle(.borderless)
            }
        }
    }

    private func managedRow(_ row: SwarmManagedList.Row, in group: SwarmManagedList.Group) -> some View {
        let label = "\(group.name), \(Self.short(row.file)), \(row.entry)"
        return VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
            HStack {
                Text(verbatim: Self.short(row.file))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(row.file)
                Text(row.entry).foregroundStyle(.secondary)
                Spacer()
                stateControl(row, label: label)
            }
            if case .changed(let found, let wrote) = row.state {
                Text(verbatim: "Found: \(found) · Swarm wrote: \(wrote) · Swarm leaves it.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            } else if row.entries.first?.recorded == false {
                Text("Matches swarm's text exactly; written before swarm kept a list.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if row.state == .off, !row.writer.hasPrefix("hooks.") {
                Text("Swarm adds it again the next time it needs it, after you agree.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.leading, DesignTokens.Size.glyphSlot + DesignTokens.Spacing.xs)
        .frame(minHeight: DesignTokens.Size.row)
        .accessibilityElement(children: .contain)
    }

    /// A switch while the row can be turned off or, for hooks, on again; else its state.
    @ViewBuilder
    private func stateControl(_ row: SwarmManagedList.Row, label: String) -> some View {
        if row.state == .on || (row.state == .off && row.writer.hasPrefix("hooks.")) {
            Toggle(label, isOn: Binding(
                get: { row.state == .on },
                set: { on in
                    if on {
                        sheet = .setUp
                    } else {
                        undo(row.presentIDs, in: row.entries)
                    }
                }
            ))
            .toggleStyle(.switch)
            .labelsHidden()
        } else {
            stateText(row.state)
        }
    }

    @ViewBuilder
    private func stateText(_ state: SwarmManagedList.RowState) -> some View {
        switch state {
        case .changed:
            Label("Changed by you", systemImage: "exclamationmark.triangle.fill")
                .symbolRenderingMode(.multicolor)
                .font(.callout)
        case .gone:
            Text("Gone").foregroundStyle(.secondary)
        case .on, .off:
            Text(state == .on ? "On" : "Off").foregroundStyle(.secondary)
        }
    }

    private func undo(_ ids: [String], in entries: [SwarmManagedList.Entry]) {
        guard !ids.isEmpty else { return }
        let hooks = entries.contains { ids.contains($0.id) && $0.writer.hasPrefix("hooks.") }
        sheet = .undo(ids: ids, hooks: hooks)
    }

    private func load() async {
        loads += 1
        let run = loads
        isLoading = true
        do {
            let loaded = try await bus.managedList()
            guard run == loads else { return }
            let made = loaded.groups
            let changes = list == nil ? [] : SwarmManagedList.changes(from: groups, to: made)
            list = loaded
            groups = made
            summary = SwarmManagedList.summary(made)
            error = nil
            if !changes.isEmpty {
                AccessibilityNotification.Announcement(changes.joined(separator: ". ")).post()
            }
        } catch {
            guard run == loads else { return }
            let message = (error as? SwarmProfileError)?.message ?? String(describing: error)
            if message != self.error {
                AccessibilityNotification.Announcement("Could not read managed changes. \(message)").post()
            }
            self.error = message
        }
        isLoading = false
    }

    private var collapsed: Set<String> {
        Set(collapsedGroups.split(separator: "\n").map(String.init))
    }

    private func toggle(_ name: String) {
        var names = collapsed
        if names.contains(name) { names.remove(name) } else { names.insert(name) }
        collapsedGroups = names.sorted().joined(separator: "\n")
    }

    private static func short(_ path: String) -> String {
        (path as NSString).abbreviatingWithTildeInPath
    }
}
