import SwiftUI
import SwarmCore

struct AgentProfilesHome: View {
    let sessionsError: String?
    let onOpenProject: () -> Void
    let onCreateProject: () -> Void
    @State private var roles: [SwarmRole] = []
    @State private var isLoading = true
    @State private var error: String?
    @State private var editing: SwarmRole?
    private let source = SwarmCLIProfileSource()
    private static var cachedRoles: [SwarmRole] = []

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            HStack {
                Button("Open Project…", action: onOpenProject)
                Button("Create Project…", action: onCreateProject)
            }
            HStack {
                Text("Agent profiles").font(.largeTitle.bold())
                Spacer()
                if isLoading && !roles.isEmpty { DelayedProgress("Refreshing profiles…") }
                Button("Refresh") {
                    isLoading = true
                    Task { await load() }
                }
                    .disabled(isLoading)
            }
            Text("These models are used when Swarm starts new agents.")
                .foregroundStyle(.secondary)
            if let sessionsError {
                Text("Chats unavailable: \(sessionsError)").foregroundStyle(.red)
            }
            if let error {
                Text(verbatim: error).foregroundStyle(.red)
            }
            if isLoading && roles.isEmpty {
                DelayedProgress("Loading profiles")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if roles.isEmpty {
                ContentUnavailableView("No agent profiles", systemImage: "person.crop.rectangle")
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(roles) { role in
                            HStack(spacing: DesignTokens.Spacing.l) {
                                VStack(alignment: .leading, spacing: DesignTokens.Spacing.xs) {
                                    Text(role.role).font(.headline)
                                    Text("\(role.provider) · \(role.runner)")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                VStack(alignment: .trailing, spacing: DesignTokens.Spacing.xs) {
                                    Text(role.model).font(DesignTokens.mono)
                                    if let effort = role.effort {
                                        Text("\(effort) effort")
                                            .font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Button("Edit") { editing = role }
                            }
                            .padding(.vertical, DesignTokens.Spacing.m)
                            Divider()
                        }
                    }
                }
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            if roles.isEmpty { roles = Self.cachedRoles }
            await load()
        }
        .sheet(item: $editing, onDismiss: { Task { await load() } }) { role in
            ModelEditSheet(
                role: role,
                affectedRoles: roles.filter { $0.runner == role.runner }.map(\.role),
                save: { try await source.setModel($0, for: role.runner) }
            )
        }
    }

    private func load() async {
        let timing = SwarmPerformance.begin("AgentProfiles")
        defer { timing.end(count: roles.count) }
        isLoading = true
        defer { isLoading = false }
        do {
            await LoginShellPath.ready()
            let loaded = try await source.roles()
            try Task.checkCancellation()
            roles = loaded
            Self.cachedRoles = loaded
            error = nil
        } catch is CancellationError {
            return
        } catch {
            self.error = (error as? SwarmProfileError)?.message ?? String(describing: error)
        }
    }
}

private struct ModelEditSheet: View {
    let role: SwarmRole
    let affectedRoles: [String]
    let save: (String) async throws -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var modelName: String
    @State private var isSaving = false
    @State private var error: String?

    init(role: SwarmRole, affectedRoles: [String], save: @escaping (String) async throws -> Void) {
        self.role = role
        self.affectedRoles = affectedRoles
        self.save = save
        _modelName = State(initialValue: role.model)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text("Edit model").font(.title2)
            LabeledContent("Profile", value: role.role)
            LabeledContent("Runner", value: role.runner)
            TextField("Model", text: $modelName)
                .textFieldStyle(.roundedBorder)
            if affectedRoles.count > 1 {
                Text("This runner is also used by \(affectedRoles.filter { $0 != role.role }.joined(separator: ", ")).")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let error { Text(verbatim: error).foregroundStyle(.red) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.disabled(isSaving)
                Button(isSaving ? "Saving…" : "Save") {
                    guard !isSaving else { return }
                    isSaving = true
                    error = nil
                    let requestedModel = modelName
                    Task {
                        defer { isSaving = false }
                        do {
                            try await save(requestedModel)
                            dismiss()
                        } catch {
                            self.error = (error as? SwarmProfileError)?.message ?? String(describing: error)
                        }
                    }
                }
                .disabled(isSaving || modelName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    || modelName == role.model)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.narrowSheet)
        .interactiveDismissDisabled(isSaving)
    }
}
