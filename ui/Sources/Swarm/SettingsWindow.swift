import SwiftUI
import SwarmCore

struct SettingsWindow: View {
    @Environment(SettingsSelection.self) private var selection
    @AppStorage("showRawData") private var showRawData = false
    @AppStorage("performanceLogging") private var performanceLogging = false
    @AppStorage("hooksSetupDeclined") private var hooksSetupDeclined = false
    @AppStorage("setupDeclined") private var setupDeclined = false
    @AppStorage("trustSetupDeclined") private var trustSetupDeclined = false
    @State private var helperVersion = "Reading helper version…"
    @State private var drift: PathSwarmDrift?

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
                if let error = selection.error {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .textSelection(.enabled).padding(DesignTokens.Spacing.m)
                }
                page
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .frame(minWidth: DesignTokens.Size.settingsWidth, minHeight: DesignTokens.Size.settingsHeight)
        .task { selection.reload() }
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
                onError: { selection.setError($0) }
            )
        case .managedChanges:
            ManagedChangesPage()
        case .setup:
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Setup").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                        .padding([.horizontal, .top], DesignTokens.Spacing.xl)
                    HooksSetupSheet(
                        loadPlan: { try await SwarmCLIBus().setupPlan($0) },
                        setUp: { digest, choice in
                            try await SwarmCLIBus().setUp(digest: digest, choice: choice)
                        },
                        notNow: { _ in }, done: {}, copy: .setup, isPage: true
                    )
                }
            }
        case .advanced:
            AdvancedSettingsPage(
                showRawData: $showRawData, performanceLogging: $performanceLogging,
                resetDeclinedPrompts: {
                    hooksSetupDeclined = false
                    setupDeclined = false
                    trustSetupDeclined = false
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
        default:
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                Text(selection.page.title).font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                if let placeholder = selection.page.placeholder { Text(placeholder).foregroundStyle(.secondary) }
            }
            .padding(DesignTokens.Spacing.xl)
        }
    }
}
