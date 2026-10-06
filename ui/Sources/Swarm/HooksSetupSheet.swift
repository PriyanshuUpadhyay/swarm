import SwiftUI
import SwarmCore

extension Notification.Name {
    /// The app menu's "Set Up Swarm…" asks the window to show the sheet.
    static let showHooksSetup = Notification.Name("SwarmShowHooksSetup")
}

/// The one consent question before swarm changes the owner's Codex and AGY config (ADR 0029). It
/// shows the plan first: each file's diff and each entry of the owner's that is in swarm's way.
/// "Set up" is open only while no conflict stands, and it sends the plan's digest, so a file that
/// changed after the owner looked is refused (ADR 0036). "Not now" writes nothing; the app menu
/// offers the same sheet later. The Managed Changes page asks its undo through the same steps with
/// the `undo` copy (ADR 0042). On start and from the app menu it shows the whole `swarm setup`
/// plan with the `setup` copy: each group with its checkbox and files, and the launch consent
/// radio under folder trust (ADR 0043, 02-design screen 1).
struct HooksSetupSheet: View {
    let loadPlan: (SwarmSetupChoice) async throws -> SwarmHooksPlan
    let setUp: (_ digest: String, SwarmSetupChoice) async throws -> Void
    let notNow: () -> Void
    let done: () -> Void
    var copy = Copy.hooks

    /// The sheet's words, so one plan and consent flow serves hooks setup and an undo.
    struct Copy {
        var question: String
        var body: String
        var loading: String
        /// What the sheet says, and VoiceOver hears, for a plan with nothing to change.
        var unchanged: String
        var blocked: String
        var apply: String
        var cancel: String
        /// Apply removes the owner's config, so it is a destructive button that Return does not
        /// press; Cancel keeps Escape.
        var destructive = false
        /// Each group gets a checkbox, and folder trust its consent radio.
        var choosesGroups = false

        static let hooks = Copy(
            question: "Let swarm set up its own hooks for Codex and AGY?",
            body: "Agents then report their chat and state to the app, so their columns show a chat and their questions. Swarm trusts only its own Codex hooks and adds its own AGY hooks; your other hooks stay as they are. Claude needs no step.",
            loading: "Reading your Codex and AGY config…",
            unchanged: "Swarm's hooks are already set up.",
            blocked: "Swarm cannot set up its hooks. Your config has entries where swarm needs its own.",
            apply: "Set up",
            cancel: "Not now"
        )

        static let setup = Copy(
            question: "Let swarm set up this Mac?",
            body: "Swarm changes only the lines below; your other entries stay. Agent hooks let Codex and AGY agents report their chat and state to the app. Folder trust lets a seat start without a trust dialog. Clear a group to leave it as it is. You can undo each change in Swarm › Managed Changes.",
            loading: "Reading your config…",
            unchanged: "Swarm is already set up.",
            blocked: "Swarm cannot set up. Your config has entries where swarm needs its own.",
            apply: "Approve and apply",
            cancel: "Not now",
            choosesGroups: true
        )

        static let undo = Copy(
            question: "Remove swarm's entries from these files?",
            body: "Swarm removes only what it wrote. Your other entries stay.",
            loading: "Reading the files…",
            unchanged: "Swarm has nothing to remove.",
            blocked: "Swarm cannot remove these yet. Each one below says what is in the way.",
            apply: "Remove",
            cancel: "Cancel",
            destructive: true
        )
    }

    private enum Phase {
        case loading
        case ready(SwarmHooksPlan)
        case failed(String)
    }

    @State private var phase = Phase.loading
    /// Each change runs the plan again.
    @State private var planRun = 0
    @State private var working = false
    @State private var failure: String?
    @State private var openFile: String?
    @State private var split = false
    @State private var diffFailed = false
    /// The groups and the consent the owner picked; each change runs the plan again.
    @State private var choice = SwarmSetupChoice()

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            switch phase {
            case .ready(let plan) where plan.isSetUp:
                Text("\(copy.unchanged) No file changes.")
                HStack {
                    Spacer()
                    Button("Done", action: done).keyboardShortcut(.defaultAction)
                }
            case .failed(let message):
                question
                Text(verbatim: message)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                HStack {
                    Spacer()
                    // A plan that keeps failing, such as on a broken config.toml, must not
                    // reopen the sheet at each launch.
                    Button(copy.cancel, action: notNow).keyboardShortcut(.cancelAction)
                    Button("Try again", action: checkAgain).keyboardShortcut(.defaultAction)
                }
            case .loading:
                question
                HStack(spacing: DesignTokens.Spacing.s) {
                    ProgressView().controlSize(.small)
                    Text(copy.loading).foregroundStyle(.secondary)
                }
                buttons(nil)
            case .ready(let plan):
                question
                if !plan.conflicts.isEmpty { conflictList(plan.conflicts) }
                if copy.choosesGroups && plan.files.allSatisfy({ $0.group != nil }) {
                    groupList(plan)
                } else if !plan.files.isEmpty {
                    fileList(plan)
                }
                if let failure {
                    Text(verbatim: failure)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                buttons(plan)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.hooksSheet)
        // A dismiss mid-write would hide its failure and reload the page before the write ends.
        .interactiveDismissDisabled(working)
        .task(id: planRun) {
            phase = .loading
            do {
                let plan = try await loadPlan(choice)
                // The first plan has every group, so it gives the checkboxes; a group that shows
                // up later gets one too, so no planned file goes unseen.
                for group in plan.groupIDs where !choice.groups.contains(group) {
                    choice.groups.append(group)
                }
                openFile = plan.files.first?.path
                diffFailed = false
                phase = .ready(plan)
                // One announcement, so a setup failure is not cut off by the plan that follows it.
                let summary = plan.isSetUp ? copy.unchanged : plan.summary
                Self.announce([failure, summary].compactMap { $0 }.joined(separator: " "))
            } catch is CancellationError {
                // The sheet closed or a newer plan run replaced this one.
            } catch {
                phase = .failed(Self.message(error))
                Self.announce(Self.message(error))
            }
        }
    }

    private var question: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            Text(copy.question)
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text(copy.body)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func conflictList(_ conflicts: [SwarmHooksPlan.Conflict]) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack(alignment: .firstTextBaseline) {
                Label(copy.blocked, systemImage: "exclamationmark.triangle.fill")
                    .symbolRenderingMode(.multicolor)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
                Button("Check again", action: checkAgain).keyboardShortcut(.defaultAction)
            }
            ViewThatFits(in: .vertical) {
                conflictRows(conflicts)
                ScrollView { conflictRows(conflicts) }
            }
            .frame(maxHeight: DesignTokens.Size.conflictList)
        }
    }

    private func conflictRows(_ conflicts: [SwarmHooksPlan.Conflict]) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            ForEach(conflicts) { conflict in
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                    Text(verbatim: "\(Self.short(conflict.file)) — \(conflict.entry)")
                        .font(.callout.weight(.medium))
                    Group {
                        Text(verbatim: "Found: \(conflict.found)")
                        Text(verbatim: "\(conflict.wantedLabel): \(conflict.wanted)")
                    }
                    .lineLimit(3)
                    .truncationMode(.middle)
                    Text(verbatim: "Fix: \(conflict.fix)")
                }
                .font(.caption)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
            }
        }
    }

    private func fileList(_ plan: SwarmHooksPlan) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack {
                Text(plan.conflicts.isEmpty
                    ? "Swarm will change these files. Nothing else changes."
                    : "After you fix each conflict, swarm will change these files:")
                Spacer()
                Picker("Diff layout", selection: $split) {
                    Text("Unified").tag(false)
                    Text("Split").tag(true)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: DesignTokens.Size.segmentedPicker)
            }
            // One file open at a time keeps the sheet inside the window.
            ForEach(plan.files) { file in fileRow(file, label: "\(Self.group(file.group))\(Self.short(file.path))") }
        }
    }

    private func fileRow(_ file: SwarmHooksPlan.File, label: String) -> some View {
        DisclosureGroup(isExpanded: Binding(
            get: { openFile == file.path },
            set: { open in
                openFile = open ? file.path : nil
                diffFailed = false
            }
        )) {
            if openFile == file.path { diff(file) }
        } label: {
            Label("\(label) · +\(file.added) −\(file.removed)", systemImage: "doc.text")
                .font(.callout)
                .help(file.path)
        }
    }

    /// Each group with its checkbox and its files, and the consent radio under folder trust. The
    /// last checked group keeps its box, so the plan always has a group to apply.
    private func groupList(_ plan: SwarmHooksPlan) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack {
                Text("Swarm changes the files of each checked group. Nothing else changes.")
                Spacer()
                Picker("Diff layout", selection: $split) {
                    Text("Unified").tag(false)
                    Text("Split").tag(true)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: DesignTokens.Size.segmentedPicker)
            }
            ForEach(choice.groups, id: \.self) { group in
                let checked = !choice.unchecked.contains(group)
                Toggle(Self.title(group), isOn: Binding(
                    get: { checked },
                    set: { on in
                        if on { choice.unchecked.remove(group) } else { choice.unchecked.insert(group) }
                        checkAgain()
                    }
                ))
                .toggleStyle(.checkbox)
                .disabled(working || (checked && choice.checked.count == 1))
                if checked {
                    VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                        if group == "trust" { consentPicker }
                        ForEach(plan.files.filter { $0.group == group }) { file in
                            fileRow(file, label: Self.short(file.path))
                        }
                    }
                    .padding(.leading, DesignTokens.Spacing.l)
                }
            }
        }
    }

    private var consentPicker: some View {
        Picker("Launch consent", selection: Binding(
            get: { choice.standing },
            set: { standing in
                choice.standing = standing
                checkAgain()
            }
        )) {
            Text("Trust each git repo and swarm scratch folder that passes the safety check, for Claude, Codex, and AGY, from now on")
                .tag(true)
            Text("Ask in the agent's column for each new folder").tag(false)
        }
        .pickerStyle(.radioGroup)
        .labelsHidden()
        .disabled(working)
        .fixedSize(horizontal: false, vertical: true)
    }

    /// The diff viewer, or the plain diff text when the viewer cannot load, because the owner
    /// must see each line before consent.
    @ViewBuilder
    private func diff(_ file: SwarmHooksPlan.File) -> some View {
        if diffFailed {
            ScrollView {
                Text(verbatim: file.diff)
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(height: DesignTokens.Size.outputPreview)
        } else {
            DiffWebView(text: file.patch, isDiff: true, split: split) { rendered in
                diffFailed = !rendered
            }
            .frame(height: DesignTokens.Size.outputPreview)
        }
    }

    private func buttons(_ plan: SwarmHooksPlan?) -> some View {
        HStack {
            Spacer()
            // While setup writes, "Not now" would record a decline for files being set up.
            Button(copy.cancel, action: notNow)
                .keyboardShortcut(.cancelAction)
                .disabled(working)
            Button(role: copy.destructive ? .destructive : nil) {
                if let plan { apply(plan) }
            } label: {
                // The label keeps its size and its name while the spinner shows.
                Text(copy.apply)
                    .opacity(working ? 0 : 1)
                    .overlay { if working { ProgressView().controlSize(.small) } }
            }
            .accessibilityLabel(copy.apply)
            .keyboardShortcut(plan?.canApply == true && !copy.destructive ? .defaultAction : nil)
            .disabled(working || plan?.canApply != true)
        }
    }

    private func checkAgain() {
        failure = nil
        planRun += 1
    }

    private func apply(_ plan: SwarmHooksPlan) {
        working = true
        failure = nil
        Task {
            do {
                try await setUp(plan.digest, choice)
                done()
            } catch {
                // The plan runs again, so a file that changed shows its new diff.
                failure = Self.message(error)
                planRun += 1
            }
            working = false
        }
    }

    /// VoiceOver hears a result that replaces what it was reading.
    private static func announce(_ text: String) {
        AccessibilityNotification.Announcement(text).post()
    }

    private static func message(_ error: any Error) -> String {
        (error as? SwarmProfileError)?.message ?? String(describing: error)
    }

    /// A `swarm setup` group's name before its file; a group this build does not know shows by
    /// its id (ADR 0043).
    private static func group(_ id: String?) -> String {
        id.map { "\(title($0)) · " } ?? ""
    }

    private static func title(_ id: String) -> String {
        switch id {
        case "hooks": "Agent hooks"
        case "trust": "Folder trust"
        case "herdr": "Herdr"
        default: id
        }
    }

    private static func short(_ path: String) -> String {
        (path as NSString).abbreviatingWithTildeInPath
    }
}
