import SwiftUI
import SwarmCore

/// Every agent profile, one line each, grouped by the name before the first dot, `chat` first.
/// A line shows the runners in order as chips and the profile's health. The list shows at once
/// from the config; the chip marks and health fill in when the slower launch check returns.
struct AgentProfilesHome: View {
    let sessionsError: String?
    let onOpenProject: () -> Void
    let onCreateProject: () -> Void
    @State private var list: SwarmProfileList?
    @State private var checks: [String: SwarmProfileCheck] = [:]
    @State private var checkedAt: Date?
    @State private var providers: [SwarmProvider] = []
    @State private var isLoading = true
    @State private var error: String?
    @State private var editing: EditTarget?
    @State private var problemsOnly = false
    @AppStorage("profiles.importBanner.dismissedFrom") private var dismissedImport = ""
    /// Closed group names, joined by commas; a group is open by default.
    @AppStorage("profiles.collapsedGroups") private var collapsedGroups = ""
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private let source = SwarmCLIProfileSource()
    private static var cachedList: SwarmProfileList?

    private struct EditTarget: Identifiable {
        let profile: SwarmProfile
        let focus: Int?
        var id: String { profile.name }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            HStack {
                Button("Open Project…", action: onOpenProject)
                Button("Create Project…", action: onCreateProject)
            }
            header
            if let sessionsError {
                Text("Chats unavailable: \(sessionsError)").foregroundStyle(.red)
            }
            if let error {
                HStack {
                    Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
                    Button("Retry") { Task { await load() } }
                }
            }
            if let imported = list?.imported, imported.from != dismissedImport {
                importNotice(imported)
            }
            if let list, !list.profiles.isEmpty {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(shownGroups(list.profiles)) { group in
                            groupSection(group.name, rows: group.profiles)
                        }
                    }
                }
            } else if isLoading {
                DelayedProgress("Loading profiles")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ContentUnavailableView {
                    Label("No agent profiles", systemImage: "person.crop.rectangle")
                } actions: {
                    Button("Retry") { Task { await load() } }
                }
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            if list == nil { list = Self.cachedList }
            await load()
        }
        .sheet(item: $editing) { target in
            ProfileEditorSheet(
                profile: target.profile,
                providers: providers,
                check: checks[target.profile.name],
                focus: target.focus,
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

    private var statuses: [ProfileStatus?] {
        (list?.profiles ?? []).map(status)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
            HStack {
                Text("Agent profiles").font(.largeTitle.bold())
                Spacer()
                if isLoading && list != nil { DelayedProgress("Checking profiles…") }
                if let checkedAt {
                    Text("Checked \(checkedAt.formatted(date: .omitted, time: .shortened))")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Button { Task { await load() } } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .disabled(isLoading)
                .help("Check the profiles again")
                .accessibilityLabel("Refresh")
            }
            Text("Swarm starts a new agent with the first runner of its profile that can run.")
                .foregroundStyle(.secondary)
            if !checks.isEmpty {
                let health = ProfileHealth(statuses)
                HStack {
                    Text("\(health.ready) ready · \(health.fallback) on fallback · \(health.blocked) blocked")
                        .font(.callout)
                    Spacer()
                    Toggle("Show problems only", isOn: $problemsOnly)
                        .toggleStyle(.checkbox)
                        .disabled(health.fallback + health.blocked == 0 && !problemsOnly)
                }
            }
        }
    }

    private func groupSection(_ name: String, rows: [SwarmProfile]) -> some View {
        let isOpen = !collapsed.contains(name)
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(reduceMotion ? nil : DesignTokens.spring) { toggle(name) }
            } label: {
                HStack(spacing: DesignTokens.Spacing.xs) {
                    Image(systemName: "chevron.right")
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                        .frame(width: DesignTokens.Size.glyphSlot)
                    Text(name).font(.subheadline.weight(.semibold))
                    Text("\(rows.count)").font(.caption).foregroundStyle(.tertiary)
                }
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: DesignTokens.Size.row, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(name) group, \(rows.count) profiles")
            .accessibilityValue(isOpen ? "open" : "closed")
            if isOpen {
                ForEach(rows) { profile in
                    ProfileRow(
                        profile: profile,
                        check: checks[profile.name],
                        status: status(profile),
                        providers: providers,
                        onEdit: { editing = EditTarget(profile: profile, focus: $0) }
                    )
                    Divider()
                }
            }
        }
    }

    /// The groups with their shown rows; a group with none is left out.
    private func shownGroups(_ profiles: [SwarmProfile]) -> [ProfileGroup] {
        ProfileGroup.groups(profiles.filter { !problemsOnly || isProblem($0) })
    }

    private var collapsed: Set<String> {
        Set(collapsedGroups.split(separator: ",").map(String.init))
    }

    private func toggle(_ group: String) {
        var names = collapsed
        if names.remove(group) == nil { names.insert(group) }
        collapsedGroups = names.sorted().joined(separator: ",")
    }

    private func status(_ profile: SwarmProfile) -> ProfileStatus? {
        ProfileStatus(check: checks[profile.name], profile: profile)
    }

    private func isProblem(_ profile: SwarmProfile) -> Bool {
        status(profile).map { $0.kind != .primary } ?? false
    }

    private func importNotice(_ imported: SwarmProfileImport) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: DesignTokens.Spacing.s) {
            Image(systemName: "info.circle").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                Text("Imported \(list?.profiles.count ?? 0) profiles from \(imported.from). The old file is not changed.")
                    .foregroundStyle(.secondary)
                if !imported.unmapped.isEmpty {
                    Text("Not imported: \(imported.unmapped.joined(separator: ", "))")
                        .foregroundStyle(.orange)
                }
            }
            Spacer()
            Button {
                dismissedImport = imported.from
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Dismiss import notice")
        }
        .font(.callout)
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
            // A check marks runners by index, so an old check on a changed file marks wrong chips.
            if loaded.revision != list?.revision {
                checks = [:]
                checkedAt = nil
            }
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
        // A failed or slow check leaves the health unknown rather than showing a wrong one.
        guard let checked = try? await source.check() else {
            checks = [:]
            checkedAt = nil
            return
        }
        checks = Dictionary(checked.map { ($0.name, $0) }, uniquingKeysWith: { first, _ in first })
        checkedAt = .now
    }
}

private struct ProfileRow: View {
    let profile: SwarmProfile
    let check: SwarmProfileCheck?
    let status: ProfileStatus?
    let providers: [SwarmProvider]
    /// Opens the editor, focused on a runner when a chip was clicked.
    let onEdit: (Int?) -> Void
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: DesignTokens.Spacing.m) {
            Text(profile.name)
                .fontWeight(.medium)
                .lineLimit(1)
                .frame(width: DesignTokens.Size.profileName, alignment: .leading)
            RunnerChain(runners: profile.runners, check: check, providers: providers, onSelect: onEdit)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button { onEdit(nil) } label: { Image(systemName: "pencil") }
                .buttonStyle(.borderless)
                .opacity(isHovered ? 1 : 0)
                .help("Edit \(profile.name)")
                .accessibilityLabel("Edit \(profile.name)")
            ProfileHealthPill(status: status)
                .frame(width: DesignTokens.Size.healthPill, alignment: .leading)
        }
        .padding(.horizontal, DesignTokens.Spacing.s)
        .frame(minHeight: DesignTokens.Size.row + DesignTokens.Spacing.xs)
        .background(status?.fill ?? .clear)
        .background(isHovered ? DesignTokens.selectionFill : .clear)
        .contentShape(Rectangle())
        .onTapGesture { onEdit(nil) }
        .onHover { isHovered = $0 }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(accessibilityText)
        .accessibilityAction(named: "Edit") { onEdit(nil) }
    }

    private var accessibilityText: String {
        guard let status else { return profile.name }
        return "\(profile.name), \(status.title.lowercased()), \(status.text)"
    }
}
