import SwiftUI
import SwarmCore

extension Notification.Name {
    /// The app menu's "Set Up Agent Hooks…" asks the window to show the sheet.
    static let showHooksSetup = Notification.Name("SwarmShowHooksSetup")
}

/// The one consent question before swarm changes the owner's Codex and AGY config (ADR 0029). It
/// shows the plan first: each file's diff and each entry of the owner's that is in swarm's way.
/// "Set up" is open only while no conflict stands, and it sends the plan's digest, so a file that
/// changed after the owner looked is refused (ADR 0035). "Not now" writes nothing; the app menu
/// offers the same sheet later.
struct HooksSetupSheet: View {
    let loadPlan: () async throws -> SwarmHooksPlan
    let setUp: (_ digest: String) async throws -> Void
    let notNow: () -> Void
    let done: () -> Void

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

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
            switch phase {
            case .ready(let plan) where plan.isSetUp:
                Text("Swarm's hooks are already set up. No file changes.")
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
                    Button("Not now", action: notNow).keyboardShortcut(.cancelAction)
                    Button("Try again", action: checkAgain).keyboardShortcut(.defaultAction)
                }
            case .loading:
                question
                HStack(spacing: DesignTokens.Spacing.s) {
                    ProgressView().controlSize(.small)
                    Text("Reading your Codex and AGY config…").foregroundStyle(.secondary)
                }
                buttons(nil)
            case .ready(let plan):
                question
                if !plan.conflicts.isEmpty { conflictList(plan.conflicts) }
                if !plan.files.isEmpty { fileList(plan) }
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
        .task(id: planRun) {
            phase = .loading
            do {
                let plan = try await loadPlan()
                openFile = plan.files.first?.path
                diffFailed = false
                phase = .ready(plan)
                // One announcement, so a setup failure is not cut off by the plan that follows it.
                Self.announce([failure, plan.summary].compactMap { $0 }.joined(separator: " "))
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
            Text("Let swarm set up its own hooks for Codex and AGY?")
                .font(.headline)
                .accessibilityAddTraits(.isHeader)
            Text("Agents then report their chat and state to the app, so their columns show a chat and their questions. Swarm trusts only its own Codex hooks and adds its own AGY hooks; your other hooks stay as they are. Claude needs no step.")
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func conflictList(_ conflicts: [SwarmHooksPlan.Conflict]) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            HStack(alignment: .firstTextBaseline) {
                Label("Swarm cannot set up its hooks. Your config has entries where swarm needs its own.", systemImage: "exclamationmark.triangle.fill")
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
                        Text(verbatim: "Swarm needs: \(conflict.wanted)")
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
            ForEach(plan.files) { file in
                DisclosureGroup(isExpanded: Binding(
                    get: { openFile == file.path },
                    set: { open in
                        openFile = open ? file.path : nil
                        diffFailed = false
                    }
                )) {
                    if openFile == file.path { diff(file) }
                } label: {
                    Label("\(Self.short(file.path)) · +\(file.added) −\(file.removed)", systemImage: "doc.text")
                        .font(.callout)
                        .help(file.path)
                }
            }
        }
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
            Button("Not now", action: notNow)
                .keyboardShortcut(.cancelAction)
                .disabled(working)
            Button {
                if let plan { apply(plan) }
            } label: {
                // The label keeps its size and its name while the spinner shows.
                Text("Set up")
                    .opacity(working ? 0 : 1)
                    .overlay { if working { ProgressView().controlSize(.small) } }
            }
            .accessibilityLabel("Set up")
            .keyboardShortcut(plan?.canApply == true ? .defaultAction : nil)
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
                try await setUp(plan.digest)
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

    private static func short(_ path: String) -> String {
        (path as NSString).abbreviatingWithTildeInPath
    }
}
