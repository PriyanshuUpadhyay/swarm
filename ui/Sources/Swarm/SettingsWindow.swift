import SwiftUI
import SwarmCore

struct SettingsWindow: View {
    @Environment(\.designTokens) private var tokens
    @Environment(SettingsSelection.self) private var selection
    @Environment(SessionsTreeModel.self) private var model
    @Environment(\.splitDiff) private var splitDiff
    @AppStorage("showRawData") private var showRawData = false
    @AppStorage("performanceLogging") private var performanceLogging = false
    @AppStorage("setupDeclined") private var setupDeclined = false
    @State private var helperVersion = "Reading helper version…"
    @State private var drift: PathSwarmDrift?
    @State private var dependencies: [DependencyRow] = []
    @State private var setupError: String?
    @State private var guardsError: String?

    var body: some View {
        NavigationSplitView {
            List(SettingsPage.allCases, selection: Binding<SettingsPage?>(
                get: { selection.page },
                set: { if let page = $0 { selection.select(page) } }
            )) { page in
                Label(page.title, systemImage: page.symbol).tag(page)
            }
            .navigationSplitViewColumnWidth(DesignTokens.Size.settingsSidebar)
        } detail: {
            VStack(alignment: .leading, spacing: 0) {
                if let error = selection.appError {
                    HStack {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .textSelection(.enabled)
                        Spacer()
                        Button("Dismiss") { selection.setAppError(nil) }
                    }
                    .foregroundStyle(.red).padding(tokens.spacing.m)
                }
                if let error = selection.loadError {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                        .textSelection(.enabled).padding(tokens.spacing.m)
                }
                if let error = selection.saveError {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                        .textSelection(.enabled).padding(tokens.spacing.m)
                }
                if let error = selection.pageError {
                    HStack {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .textSelection(.enabled)
                        if selection.page == .setup, selection.skillsRefreshError != nil {
                            Spacer()
                            Button("Retry") {
                                Task { _ = await selection.retrySkillsRefresh() }
                            }
                            .disabled(selection.skillsRefreshing)
                        }
                    }
                    .foregroundStyle(.red).padding(tokens.spacing.m)
                }
                page
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .frame(minWidth: DesignTokens.Size.settingsWidth, minHeight: DesignTokens.Size.settingsHeight)
        .onAppear {
            setupError = nil
            guardsError = nil
            selection.setPageError(nil, on: .setup)
        }
        .task { selection.reload() }
        .onChange(of: selection.appErrorRevision, initial: true) { _, _ in
            if let message = selection.appError { AccessibilityNotification.Announcement(message).post() }
        }
        .onChange(of: selection.loadErrorRevision) { _, _ in
            if let message = selection.loadError { AccessibilityNotification.Announcement(message).post() }
        }
        .onChange(of: selection.saveErrorRevision, initial: true) { _, _ in
            if let message = selection.saveError { AccessibilityNotification.Announcement(message).post() }
        }
        .onChange(of: selection.pageErrorRevision, initial: true) { _, _ in
            if let message = selection.pageError { AccessibilityNotification.Announcement(message).post() }
        }
        .onChange(of: selection.page) { _, page in
            if page != .setup {
                setupError = nil
                guardsError = nil
            }
        }
        .onChange(of: selection.prefs.notices) { _, _ in model.updateDockBadge() }
    }

    private var setupErrorMessage: String? {
        ErrorAnnouncement.joined([guardsError, setupError])
    }

    private func setSetupError(_ message: String?) {
        let previous = setupErrorMessage
        setupError = message
        if message != nil, let joined = setupErrorMessage {
            selection.reportPageError(joined, on: .setup)
        } else if setupErrorMessage != previous {
            selection.setPageError(setupErrorMessage, on: .setup)
        }
    }

    private func setGuardsError(_ message: String?) {
        let previous = setupErrorMessage
        guardsError = message
        if setupErrorMessage != previous { selection.setPageError(setupErrorMessage, on: .setup) }
    }

    private func reportGuardsError(_ message: String) {
        guardsError = message
        if let joined = setupErrorMessage { selection.reportPageError(joined, on: .setup) }
    }

    @ViewBuilder
    private var page: some View {
        switch selection.page {
        case .profiles:
            ProfilesPage(
                cachedProfiles: { await SwarmProfileCatalog.shared.cachedProfiles },
                loadProfiles: {
                    await LoginShellPath.ready()
                    return try await SwarmProfileCatalog.shared.profiles()
                },
                loadProviders: { try await SwarmProfileCatalog.shared.providers() },
                checkProfiles: { try await SwarmCLIProfileSource().check() },
                loadModels: { try await SwarmModelCatalog.shared.models(for: $0) },
                saveProfile: { profile, revision in
                    try await SwarmCLIProfileSource().save(profile, revision: revision)
                },
                profileAction: { action, revision in
                    try await SwarmCLIBus().profileAction(action, revision: revision)
                },
                onError: { message in
                    if let message { selection.reportPageError(message, on: .profiles) }
                    else { selection.setPageError(nil, on: .profiles) }
                }
            )
        case .accounts:
            AccountsPage(
                loadAccounts: { provider in
                    await LoginShellPath.ready()
                    if provider != "agy", Shell.which(provider) == nil {
                        throw AccountsPageError.cliMissing(provider)
                    }
                    return try await SwarmCLIProfileSource().accounts(provider: provider)
                },
                loadUsage: {
                    await LoginShellPath.ready()
                    return try await SwarmCLIProfileSource().usage()
                },
                refreshUsage: { provider in
                    await LoginShellPath.ready()
                    return try await SwarmCLIProfileSource().refreshUsage(provider: provider)
                },
                openLogin: { request in
                    await LoginShellPath.ready()
                    var request = request
                    request.directory = model.navigation.selectedWorkspace ?? NSHomeDirectory()
                    return try await SwarmCLIProfileSource().openLogin(request)
                },
                resetAccounts: { revision in
                    try await SwarmCLIProfileSource().resetAccounts(revision: revision)
                },
                onError: { message in
                    if let message { selection.reportPageError(message, on: .accounts) }
                    else { selection.setPageError(nil, on: .accounts) }
                }
            )
        case .managedChanges:
            ManagedChangesPage()
        case .setup:
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Setup").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                        .padding([.horizontal, .top], tokens.spacing.xl)
                    DependenciesGroup(rows: dependencies)
                        .padding([.horizontal, .top], tokens.spacing.xl)
                        .task {
                            await LoginShellPath.ready()
                            dependencies = Dependencies.check(lookup: Shell.which)
                        }
                    HooksSetupSheet(
                        loadPlan: {
                            _ = await selection.waitForSkillsRefresh()
                            return try await SwarmCLIBus().setupPlan($0)
                        },
                        setUp: { digest, choice in
                            _ = await selection.waitForSkillsRefresh()
                            try await SwarmCLIBus().setUp(digest: digest, choice: choice)
                        },
                        notNow: { _ in }, done: {}, copy: .setup,
                        onError: setSetupError,
                        canAnnounceSummary: { setupError == nil }
                    )
                    .id(selection.skillsReady)
                    GuardsPage(
                        load: {
                            do {
                                return GuardRules.load(url: try GuardRules.fileURL(environment: ProcessInfo.processInfo.environment))
                            } catch {
                                return .failure(GuardListError(reason: error.localizedDescription))
                            }
                        },
                        save: { editor in
                            try editor.save(to: GuardRules.fileURL(environment: ProcessInfo.processInfo.environment))
                        },
                        onError: setGuardsError,
                        reportError: reportGuardsError
                    )
                    .padding([.horizontal, .bottom], tokens.spacing.xl)
                }
            }
        case .advanced:
            AdvancedSettingsPage(
                showRawData: $showRawData, performanceLogging: $performanceLogging,
                resetDeclinedPrompts: {
                    setupDeclined = false
                    for key in SetupGroup.allCases.compactMap(\.declineFlagKey) {
                        UserDefaults.standard.set(false, forKey: key)
                    }
                },
                dataHome: SwarmHome.dataFolder?.path,
                revealDataHome: {
                    if let folder = SwarmHome.dataFolder {
                        NSWorkspace.shared.activateFileViewerSelecting([folder])
                    }
                },
                helperVersion: helperVersion, drift: drift
            )
            .task {
                await LoginShellPath.ready()
                helperVersion = await PathSwarmCheck.helperVersion() ?? "Helper version unavailable"
                drift = await PathSwarmCheck.current(dismissed: [])
            }
        case .keys:
            VStack(alignment: .leading, spacing: tokens.spacing.m) {
                Text("Keys").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                Grid(alignment: .leading, horizontalSpacing: tokens.spacing.xl,
                     verticalSpacing: tokens.spacing.s) {
                    ForEach(KeysPage.rows, id: \.shortcut) { row in
                        GridRow {
                            Text(row.shortcut).monospaced()
                            Text(row.title)
                        }
                    }
                }
                Text("A custom keymap comes later.").foregroundStyle(.secondary)
            }
            .padding(tokens.spacing.xl)
        case .appearance:
            VStack(alignment: .leading, spacing: tokens.spacing.m) {
                Text("Appearance").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                Picker("Theme", selection: Binding(get: { selection.prefs.theme }, set: selection.setTheme)) {
                    Text("System").tag(Theme.system)
                    Text("Light").tag(Theme.light)
                    Text("Dark").tag(Theme.dark)
                }
                Picker("Text size", selection: Binding(get: { selection.prefs.textSize }, set: selection.setTextSize)) {
                    Text("Small").tag(TextSize.small)
                    Text("Default").tag(TextSize.default)
                    Text("Large").tag(TextSize.large)
                }
                Picker("Density", selection: Binding(get: { selection.prefs.density }, set: selection.setDensity)) {
                    Text("Comfortable").tag(Density.comfortable)
                    Text("Compact").tag(Density.compact)
                }
                Picker("Diff layout", selection: splitDiff) {
                    Text("Unified").tag(false)
                    Text("Split").tag(true)
                }
                .pickerStyle(.segmented)
                .frame(width: DesignTokens.Size.segmentedPicker)
                Picker("Send with", selection: Binding(get: { selection.prefs.sendKey }, set: selection.setSendKey)) {
                    Text("Return").tag(SendKey.return)
                    Text("⌘Return").tag(SendKey.commandReturn)
                }
            }
            .padding(tokens.spacing.xl)
        case .notifications:
            ScrollView {
                VStack(alignment: .leading, spacing: tokens.spacing.l) {
                    Text("Notifications").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                    Toggle("Post notices while Swarm runs", isOn: noticeBinding(\.post))
                    Toggle("Play a sound", isOn: noticeBinding(\.sound))
                    Toggle("Notify when a chat is done", isOn: noticeBinding(\.done))
                    Toggle("Badge the Dock with chats that need input", isOn: noticeBinding(\.badge))
                    Divider()
                    Text("Mute projects").font(.headline).accessibilityAddTraits(.isHeader)
                    if model.tree.projects.isEmpty {
                        Text("Import a project to set its mute choice.").foregroundStyle(.secondary)
                    }
                    ForEach(model.tree.projects, id: \.path) { project in
                        Toggle(model.navigation.projectTitle(for: project), isOn: Binding(
                            get: { selection.prefs.notices.mutedProjects.contains(project.path) },
                            set: { selection.setProjectMuted(project.path, to: $0) }
                        ))
                    }
                }
                .toggleStyle(.switch)
                .padding(tokens.spacing.xl)
            }
        default:
            VStack(alignment: .leading, spacing: tokens.spacing.m) {
                Text(selection.page.title).font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                if let placeholder = selection.page.placeholder { Text(placeholder).foregroundStyle(.secondary) }
            }
            .padding(tokens.spacing.xl)
        }
    }

    private func noticeBinding(_ field: WritableKeyPath<NoticePrefs, Bool>) -> Binding<Bool> {
        Binding(get: { selection.prefs.notices[keyPath: field] },
                set: { selection.setNotice(field, to: $0) })
    }
}
