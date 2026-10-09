import SwiftUI
import SwarmCore

/// Settings > Managed Changes: each item swarm wrote outside its home, one row per writer and file
/// (ADR 0042). A row's switch is its undo: off shows the removal's diff in the hooks setup sheet
/// before anything is written, and on runs the writer's own plan again. The page holds no write
/// logic; each write is the CLI behind that sheet (ADR 0036).
struct ManagedChangesPage: View {
    @Environment(\.designTokens) private var tokens
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
    @AppStorage("skillsSetupDeclined") private var skillsSetupDeclined = false
    @Environment(SettingsSelection.self) private var selection
    /// Closed group names, joined by newlines; a group is open by default.
    @AppStorage("managed.collapsedGroups") private var collapsedGroups = ""
    @Environment(\.controlActiveState) private var activeState
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let bus = SwarmCLIBus()

    private enum Sheet: Identifiable {
        case undo(ids: [String], hooks: Bool, skills: Bool)
        case setUp(group: String)

        var id: String {
            switch self {
            case .undo(let ids, _, _): ids.joined(separator: ",")
            case .setUp(let group): "setUp-" + group
            }
        }
    }

    var body: some View {
        let collapsed = self.collapsed
        VStack(alignment: .leading, spacing: tokens.spacing.l) {
            header
            if let error {
                HStack {
                    // Red only on the symbol: red body text is near 3.5:1 on white, below 4.5:1.
                    Label {
                        Text(verbatim: error).textSelection(.enabled)
                    } icon: {
                        Image(systemName: "xmark.octagon.fill").foregroundStyle(.red)
                    }
                    // Each click is a full CLI read of every managed file, so one runs at a time.
                    Button("Retry") { Task { await load(retry: true) } }
                        .disabled(isLoading)
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
        .padding(tokens.spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task { await load() }
        .onChange(of: activeState) { _, state in
            if state == .key { Task { await load() } }
        }
        .sheet(item: $sheet, onDismiss: { Task { await load() } }) { kind in
            switch kind {
            case .undo(let ids, let hooks, let skills):
                HooksSetupSheet(
                    loadPlan: { _ in try await bus.managedRevertPlan(ids: ids) },
                    setUp: { digest, _ in
                        try await bus.revertManaged(ids: ids, digest: digest)
                        if hooks { hooksSetupDeclined = true }
                        if skills { skillsSetupDeclined = true }
                    },
                    notNow: { _ in sheet = nil },
                    done: { sheet = nil },
                    copy: .undo
                )
            case .setUp(let group):
                HooksSetupSheet(
                    loadPlan: { _ in
                        if group == "skills" {
                            _ = await selection.waitForSkillsRefresh()
                            return try await bus.setupPlan(.skillsOnly())
                        }
                        return try await bus.hooksPlan()
                    },
                    setUp: { digest, _ in
                        if group == "skills" {
                            _ = await selection.waitForSkillsRefresh()
                            try await bus.setUp(digest: digest, choice: .skillsOnly())
                            skillsSetupDeclined = false
                        } else {
                            try await bus.setUpHooks(digest: digest)
                        }
                    },
                    notNow: { _ in sheet = nil },
                    done: { sheet = nil },
                    copy: group == "skills" ? .skills : .hooks
                )
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
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
                HStack(spacing: tokens.spacing.xs) {
                    Image(systemName: isOpen ? "chevron.down" : "chevron.forward")
                        .frame(width: DesignTokens.Size.glyphSlot)
                    Text(group.name).font(.subheadline.weight(.semibold))
                    if let source = group.source {
                        Text("· \(source)").font(.caption).foregroundStyle(.tertiary)
                    }
                }
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: tokens.row, alignment: .leading)
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
        return VStack(alignment: .leading, spacing: tokens.spacing.xxs) {
            HStack {
                Text(verbatim: Self.short(row.file))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(row.file)
                if row.writer != "skills" { Text(row.entry).foregroundStyle(.secondary) }
                Spacer()
                stateControl(row, label: label)
            }
            if row.writer == "skills" {
                Text(verbatim: row.entry)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            if case .changed(let found, let wrote) = row.state {
                Text(verbatim: "Found: \(found) · Swarm wrote: \(wrote) · Swarm leaves it.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            } else if case .unreadable(let reason) = row.state {
                Text(verbatim: "\(reason) · Swarm leaves it.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            } else if case .partly(let present, let total) = row.state {
                Text("Partly on: \(present) of \(total) entries. Turning it off removes the rest.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if case .unknown(let state) = row.state {
                Text("This app does not know the state \"\(state)\". Update Swarm to change this row.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if row.entries.first?.recorded == false {
                Text("Matches swarm's text exactly; written before swarm kept a list.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if row.state == .off, row.restoreGroup == nil {
                Text("Swarm adds it again the next time it needs it, after you agree.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.leading, DesignTokens.Size.glyphSlot + tokens.spacing.xs)
        .frame(minHeight: tokens.row)
        .accessibilityElement(children: .contain)
    }

    /// A switch while the row can be turned off or its writer can restore it; else its state.
    @ViewBuilder
    private func stateControl(_ row: SwarmManagedList.Row, label: String) -> some View {
        let on = switch row.state {
        case .on, .partly: true
        default: false
        }
        if case .unknown = row.state {
            // The switch keeps what swarm knows, but this build cannot tell what Off or On does.
            Toggle(label, isOn: .constant(!row.presentIDs.isEmpty))
                .toggleStyle(.switch)
                .labelsHidden()
                .disabled(true)
        } else if on || (row.state == .off && row.restoreGroup != nil) {
            Toggle(label, isOn: Binding(
                get: { on },
                set: { on in
                    if on {
                        if let group = row.restoreGroup { sheet = .setUp(group: group) }
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
        case .unreadable:
            Label("Cannot read", systemImage: "exclamationmark.triangle.fill")
                .symbolRenderingMode(.multicolor)
                .font(.callout)
        case .gone, .partly, .unknown:
            Text(state.name).foregroundStyle(.secondary)
        case .on, .off:
            Text(state == .on ? "On" : "Off").foregroundStyle(.secondary)
        }
    }

    private func undo(_ ids: [String], in entries: [SwarmManagedList.Entry]) {
        guard !ids.isEmpty else { return }
        let hooks = entries.contains { ids.contains($0.id) && $0.isHooks }
        let skills = entries.contains { ids.contains($0.id) && $0.writer == "skills" }
        sheet = .undo(ids: ids, hooks: hooks, skills: skills)
    }

    /// `retry`: the owner pressed Retry, so a failure is announced even when its text is the same.
    private func load(retry: Bool = false) async {
        loads += 1
        let run = loads
        isLoading = true
        // The page's spinner shows while there are no groups and the read passes its delay; the
        // list then replaces the spinner that had VoiceOver's focus.
        let spinner = groups.isEmpty
        let started = ContinuousClock.now
        do {
            let loaded = try await bus.managedList()
            guard run == loads else { return }
            let made = loaded.groups
            let spun = spinner && ContinuousClock.now - started >= DelayedProgress.delay
            let spoken = SwarmManagedList.announcement(
                from: list == nil ? nil : groups, to: made, waited: error != nil || spun
            )
            list = loaded
            groups = made
            summary = SwarmManagedList.summary(made)
            error = nil
            if let spoken {
                AccessibilityNotification.Announcement(spoken).post()
            }
        } catch is CancellationError {
            // The window closed or a newer load replaced this one, so there is no error to show.
            return
        } catch {
            guard run == loads else { return }
            let message = (error as? SwarmProfileError)?.message ?? String(describing: error)
            if retry || message != self.error {
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
