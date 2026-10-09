import SwiftUI
import SwarmCore

/// Every agent profile, grouped by the name before the first dot, `chat` first.
/// A row shows its runners, health and actions. The list shows at once
/// from the config; the chip marks and health fill in when the slower launch check returns.
struct ProfilesPage: View {
    @Environment(\.designTokens) private var tokens
    let cachedProfiles: () async -> SwarmProfileList?
    let loadProfiles: () async throws -> SwarmProfileList
    let loadProviders: () async throws -> [SwarmProvider]
    let checkProfiles: () async throws -> [SwarmProfileCheck]
    let loadModels: (String) async throws -> [SwarmModel]
    let saveProfile: (SwarmProfile, String) async throws -> String
    let profileAction: (ProfileAction, String) async throws -> String
    let onError: (String?) -> Void
    @State private var list: SwarmProfileList?
    @State private var checks: [String: SwarmProfileCheck] = [:]
    @State private var checkedAt: Date?
    @State private var providers: [SwarmProvider] = []
    @State private var providersError: String?
    @State private var isLoading = true
    @State private var editing: EditTarget?
    @State private var problemsOnly = false
    @State private var isActing = false
    @State private var nameAction: NameAction?
    @State private var profileName = ""
    @State private var confirmation: Confirmation?
    @State private var usagePct = ""
    /// The newest load; an older load that answers late writes nothing.
    @State private var loads = 0
    @AppStorage("profiles.importBanner.dismissedFrom") private var dismissedImport = ""
    /// Closed group names, joined by commas; a group is open by default.
    @AppStorage("profiles.collapsedGroups") private var collapsedGroups = ""
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private enum NameAction {
        case new, rename(String), copy(String)

        var title: String {
            switch self {
            case .new: "New profile"
            case .rename(let name): "Rename \(name)"
            case .copy(let name): "Copy \(name)"
            }
        }

        var button: String {
            switch self {
            case .new: "Create"
            case .rename: "Rename"
            case .copy: "Copy"
            }
        }

        func action(name: String) -> ProfileAction {
            switch self {
            case .new: .new(name)
            case .rename(let from): .rename(from: from, to: name)
            case .copy(let from): .copy(from: from, to: name)
            }
        }
    }

    private enum Confirmation {
        case delete(String), reset

        var action: ProfileAction {
            switch self {
            case .delete(let name): .delete(name)
            case .reset: .reset
            }
        }
    }

    private struct EditTarget: Identifiable {
        let profile: SwarmProfile
        let focus: Int?
        var id: String { profile.name }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.l) {
            header
            actionControls
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
        .padding(tokens.spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            if list == nil { list = await cachedProfiles() }
            await load()
        }
        .sheet(item: $editing) { target in
            ProfileEditorSheet(
                profile: target.profile,
                providers: providers,
                providersError: providersError,
                check: checks[target.profile.name],
                focus: target.focus,
                models: loadModels,
                save: { edited in
                    guard let revision = list?.revision else {
                        throw SwarmProfileError.failed("Profiles are not loaded yet")
                    }
                    _ = try await saveProfile(edited, revision)
                },
                onSaved: { Task { await load() } },
                onError: onError
            )
        }
    }

    private var statuses: [ProfileStatus?] {
        (list?.profiles ?? []).map(status)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
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
                .disabled(isLoading || isActing)
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

    private var actionControls: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.s) {
            HStack {
                Button("New…") { startNameAction(.new) }
                Button("Reset to bundled") {
                    nameAction = nil
                    confirmation = .reset
                }
            }
            if let nameAction {
                VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                    HStack {
                        TextField(nameAction.title, text: $profileName)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit { submitName() }
                        Button(nameAction.button) { submitName() }
                        Button("Cancel") { self.nameAction = nil }
                    }
                }
            }
            if let confirmation {
                VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                    switch confirmation {
                    case .delete(let name): Text("Delete profile '\(name)'?")
                    case .reset:
                        Text("Reset to bundled will replace these profiles: \(profileNames.joined(separator: ", ")).")
                    }
                    HStack {
                        Button("Cancel") { self.confirmation = nil }
                        Button(confirmationButton, role: .destructive) { perform(confirmation.action) }
                    }
                }
            }
            HStack(spacing: tokens.spacing.xs) {
                Text("Skip a runner when its usage left is below")
                TextField("Usage left", text: $usagePct, onEditingChanged: { editing in
                    if !editing { commitUsage() }
                })
                .textFieldStyle(.roundedBorder)
                .frame(width: DesignTokens.Size.usagePctField)
                .onSubmit { commitUsage() }
                Text("%")
            }
        }
        .disabled(list == nil || isLoading || isActing)
    }

    private var profileNames: [String] { list?.profiles.map(\.name) ?? [] }

    private var confirmationButton: String {
        if case .delete = confirmation { return "Delete" }
        return "Reset to bundled"
    }

    private func startNameAction(_ action: NameAction) {
        confirmation = nil
        nameAction = action
        switch action {
        case .new: profileName = ""
        case .rename(let name): profileName = name
        case .copy(let name): profileName = name + ".copy"
        }
    }

    private func submitName() {
        guard let nameAction else { return }
        perform(nameAction.action(name: profileName))
    }

    private func commitUsage() {
        guard let list, !isLoading, !isActing else { return }
        guard let pct = Int(usagePct) else {
            onError("Enter a whole number from 0 to 100 for usage left.")
            return
        }
        guard pct != list.minUsageLeftPct else { return }
        perform(.setMinUsage(pct))
    }

    private func perform(_ action: ProfileAction) {
        guard let revision = list?.revision, !isLoading, !isActing else { return }
        isActing = true
        onError(nil)
        Task {
            defer { isActing = false }
            do {
                _ = try await profileAction(action, revision)
                nameAction = nil
                confirmation = nil
                await load()
            } catch {
                let message = (error as? SwarmProfileError)?.message ?? String(describing: error)
                await load()
                onError(message)
            }
        }
    }

    private func groupSection(_ name: String, rows: [SwarmProfile]) -> some View {
        let isOpen = !collapsed.contains(name)
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(reduceMotion ? nil : DesignTokens.spring) { toggle(name) }
            } label: {
                HStack(spacing: tokens.spacing.xs) {
                    Image(systemName: "chevron.right")
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                        .frame(width: DesignTokens.Size.glyphSlot)
                    Text(name).font(.subheadline.weight(.semibold))
                    Text("\(rows.count)").font(.caption).foregroundStyle(.tertiary)
                }
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: tokens.row, alignment: .leading)
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
                        onEdit: { editing = EditTarget(profile: profile, focus: $0) },
                        onRename: { startNameAction(.rename(profile.name)) },
                        onCopy: { startNameAction(.copy(profile.name)) },
                        onDelete: {
                            nameAction = nil
                            confirmation = .delete(profile.name)
                        }
                    )
                    .disabled(isLoading || isActing)
                    Divider()
                }
            }
        }
    }

    /// The groups with their shown rows; a group with none is left out. With no check yet no
    /// profile is known to be a problem, so every row shows.
    private func shownGroups(_ profiles: [SwarmProfile]) -> [ProfileGroup] {
        ProfileGroup.groups(profiles.filter { !problemsOnly || checks.isEmpty || isProblem($0) })
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
        HStack(alignment: .firstTextBaseline, spacing: tokens.spacing.s) {
            Image(systemName: "info.circle").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: tokens.spacing.xxs) {
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
        loads += 1
        let generation = loads
        isLoading = true
        defer { if generation == loads { isLoading = false } }
        do {
            let loaded = try await loadProfiles()
            try Task.checkCancellation()
            guard generation == loads else { return }
            // A check marks runners by index, so an old check on a changed file marks wrong chips.
            if loaded.revision != list?.revision {
                checks = [:]
                checkedAt = nil
            }
            list = loaded
            usagePct = String(loaded.minUsageLeftPct)
            onError(nil)
        } catch is CancellationError {
            return
        } catch {
            guard generation == loads else { return }
            onError((error as? SwarmProfileError)?.message ?? String(describing: error))
            return
        }
        if providers.isEmpty {
            do {
                let loaded = try await loadProviders()
                try Task.checkCancellation()
                guard generation == loads else { return }
                providers = loaded
                providersError = nil
            } catch is CancellationError {
                return
            } catch {
                guard generation == loads else { return }
                providersError = (error as? SwarmProfileError)?.message ?? String(describing: error)
                onError(providersError)
            }
        }
        // A failed or slow check leaves the health unknown rather than showing a wrong one.
        do {
            let checked = try await checkProfiles()
            guard generation == loads else { return }
            checks = Dictionary(checked.map { ($0.name, $0) }, uniquingKeysWith: { first, _ in first })
            checkedAt = .now
        } catch {
            guard generation == loads else { return }
            checks = [:]
            checkedAt = nil
            if !(error is CancellationError) {
                onError((error as? SwarmProfileError)?.message ?? String(describing: error))
            }
        }
    }
}

private struct ProfileRow: View {
    @Environment(\.designTokens) private var tokens
    let profile: SwarmProfile
    let check: SwarmProfileCheck?
    let status: ProfileStatus?
    let providers: [SwarmProvider]
    /// Opens the editor, focused on a runner when a chip was clicked.
    let onEdit: (Int?) -> Void
    let onRename: () -> Void
    let onCopy: () -> Void
    let onDelete: () -> Void
    @State private var isHovered = false

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            row
            HStack {
                Spacer()
                Button("Rename", action: onRename).disabled(profile.name == "chat")
                Button("Copy", action: onCopy)
                Button("Delete", role: .destructive, action: onDelete).disabled(profile.name == "chat")
            }
            .buttonStyle(.borderless)
            .controlSize(.small)
        }
        .padding(tokens.spacing.s)
        .background(status?.fill ?? .clear)
        .background(isHovered ? DesignTokens.selectionFill : .clear)
        .onHover { isHovered = $0 }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(accessibilityText)
        .accessibilityAction(named: "Edit") { onEdit(nil) }
    }

    private var row: some View {
        HStack(spacing: tokens.spacing.m) {
            Text(profile.name)
                .fontWeight(.medium)
                .lineLimit(1)
                .frame(width: DesignTokens.Size.profileName, alignment: .leading)
            RunnerChain(runners: profile.runners, check: check, providers: providers, onSelect: onEdit)
                .frame(maxWidth: .infinity, alignment: .leading)
            // Always shown, so a keyboard user can Tab to it; hover only darkens it.
            Button { onEdit(nil) } label: { Image(systemName: "pencil") }
                .buttonStyle(.borderless)
                .foregroundStyle(isHovered ? .primary : .secondary)
                .help("Edit \(profile.name)")
                .accessibilityLabel("Edit \(profile.name)")
            ProfileHealthPill(status: status)
                .frame(width: DesignTokens.Size.healthPill, alignment: .leading)
        }
        .padding(.horizontal, tokens.spacing.s)
        .frame(minHeight: tokens.row + tokens.spacing.xs)
        .contentShape(Rectangle())
        .onTapGesture { onEdit(nil) }
    }

    private var accessibilityText: String {
        guard let status else { return profile.name }
        return "\(profile.name), \(status.title.lowercased()), \(status.text)"
    }
}
