import SwiftUI
import SwarmCore

/// Every agent profile, `chat` first, with its runners in order and the runner the next launch
/// takes. The list shows at once from the config; each status line fills in when the slower
/// launch check returns.
struct AgentProfilesHome: View {
    let sessionsError: String?
    let onOpenProject: () -> Void
    let onCreateProject: () -> Void
    @State private var list: SwarmProfileList?
    @State private var checks: [String: SwarmProfileCheck] = [:]
    @State private var providers: [SwarmProvider] = []
    @State private var isLoading = true
    @State private var error: String?
    @State private var editing: SwarmProfile?
    @AppStorage("profiles.importBanner.dismissedFrom") private var dismissedImport = ""
    private let source = SwarmCLIProfileSource()
    private static var cachedList: SwarmProfileList?

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            HStack {
                Button("Open Project…", action: onOpenProject)
                Button("Create Project…", action: onCreateProject)
            }
            HStack {
                Text("Agent profiles").font(.largeTitle.bold())
                Spacer()
                if isLoading && list != nil { DelayedProgress("Refreshing profiles…") }
                Button("Refresh") { Task { await load() } }
                    .disabled(isLoading)
            }
            Text("Swarm starts a new agent with the first runner of its profile that can run.")
                .foregroundStyle(.secondary)
            if let sessionsError {
                Text("Chats unavailable: \(sessionsError)").foregroundStyle(.red)
            }
            if let error {
                Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
            }
            if let imported = list?.imported, imported.from != dismissedImport {
                importBanner(imported)
            }
            if let list, !list.profiles.isEmpty {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(list.profiles) { profile in
                            ProfileRow(
                                profile: profile,
                                status: ProfileStatus(check: checks[profile.name], profile: profile),
                                onEdit: { editing = profile }
                            )
                            Divider()
                        }
                    }
                }
            } else if isLoading {
                DelayedProgress("Loading profiles")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ContentUnavailableView("No agent profiles", systemImage: "person.crop.rectangle")
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            if list == nil { list = Self.cachedList }
            await load()
        }
        .sheet(item: $editing) { profile in
            ProfileEditorSheet(
                profile: profile,
                providers: providers,
                save: { edited in
                    guard let revision = list?.revision else {
                        throw SwarmProfileError.failed("Profiles are not loaded yet")
                    }
                    _ = try await source.save(edited, revision: revision)
                },
                onSaved: { Task { await load() } }
            )
        }
    }

    private func importBanner(_ imported: SwarmProfileImport) -> some View {
        HStack(alignment: .top, spacing: DesignTokens.Spacing.s) {
            Image(systemName: "info.circle").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                Text("Imported \(list?.profiles.count ?? 0) profiles from \(imported.from).")
                Text("The old file is not changed.").foregroundStyle(.secondary)
                if !imported.unmapped.isEmpty {
                    Text("Not imported: \(imported.unmapped.joined(separator: ", "))")
                        .foregroundStyle(.orange)
                }
            }
            .font(.callout)
            Spacer()
            Button {
                dismissedImport = imported.from
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Dismiss import notice")
        }
        .padding(DesignTokens.Spacing.m)
        .background(DesignTokens.selectionFill, in: RoundedRectangle(cornerRadius: DesignTokens.Radius.card))
    }

    private func load() async {
        let timing = SwarmPerformance.begin("AgentProfiles")
        defer { timing.end(count: list?.profiles.count ?? 0) }
        isLoading = true
        defer { isLoading = false }
        do {
            await LoginShellPath.ready()
            let loaded = try await source.profiles()
            try Task.checkCancellation()
            list = loaded
            Self.cachedList = loaded
            error = nil
        } catch is CancellationError {
            return
        } catch {
            self.error = (error as? SwarmProfileError)?.message ?? String(describing: error)
            return
        }
        if providers.isEmpty { providers = (try? await source.providers()) ?? [] }
        // A failed or slow check leaves the status lines empty rather than showing a wrong one.
        let checked = (try? await source.check()) ?? []
        checks = Dictionary(checked.map { ($0.name, $0) }, uniquingKeysWith: { first, _ in first })
    }
}

private struct ProfileRow: View {
    let profile: SwarmProfile
    let status: ProfileStatus?
    let onEdit: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: DesignTokens.Spacing.l) {
            Button(action: onEdit) {
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                    Text(profile.name).font(.headline)
                    Text(chain).font(DesignTokens.mono).foregroundStyle(.secondary)
                    if let status {
                        HStack(spacing: DesignTokens.Spacing.xs) {
                            Image(systemName: symbol(status.kind))
                                .foregroundStyle(color(status.kind))
                                .accessibilityHidden(true)
                            Text(status.text)
                        }
                        .font(.caption)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(profile.name), \(chain)")
            .accessibilityHint(status?.text ?? "")
            Button("Edit", action: onEdit)
        }
        .padding(.vertical, DesignTokens.Spacing.m)
    }

    private var chain: String {
        profile.runners.map { "\($0.provider) · \($0.model) · \($0.effort)" }.joined(separator: "  →  ")
    }

    private func symbol(_ kind: ProfileStatus.Kind) -> String {
        switch kind {
        case .primary: "circle.fill"
        case .fallback: "circle.lefthalf.filled"
        case .none: "circle"
        }
    }

    private func color(_ kind: ProfileStatus.Kind) -> Color {
        switch kind {
        case .primary: .green
        case .fallback: .orange
        case .none: .red
        }
    }
}
