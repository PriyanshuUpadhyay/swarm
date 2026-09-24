import SwiftUI
import SwarmCore

struct AgentProfilesHome: View {
    let sessionsError: String?
    @State private var roles: [SwarmRole] = []
    @State private var isLoading = true
    @State private var error: String?
    @State private var editing: SwarmRole?
    private let source = SwarmCLIProfileSource()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Agent profiles").font(.largeTitle.bold())
                Spacer()
                Button("Refresh") { Task { await load() } }
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
                ProgressView("Loading profiles")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if roles.isEmpty {
                ContentUnavailableView("No agent profiles", systemImage: "person.crop.rectangle")
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(roles) { role in
                            HStack(spacing: 16) {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(role.role).font(.headline)
                                    Text("\(role.provider) · \(role.runner)")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                VStack(alignment: .trailing, spacing: 4) {
                                    Text(role.model).font(.system(.body, design: .monospaced))
                                    if let effort = role.effort {
                                        Text("\(effort) effort")
                                            .font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Button("Edit") { editing = role }
                            }
                            .padding(.vertical, 12)
                            Divider()
                        }
                    }
                }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task { await load() }
        .sheet(item: $editing, onDismiss: { Task { await load() } }) { role in
            ModelEditSheet(
                role: role,
                affectedRoles: roles.filter { $0.runner == role.runner }.map(\.role),
                save: { try await source.setModel($0, for: role.runner) }
            )
        }
    }

    private func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            await LoginShellPath.ready()
            roles = try await source.roles()
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
        VStack(alignment: .leading, spacing: 16) {
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
                Button("Cancel") { dismiss() }
                Button(isSaving ? "Saving…" : "Save") {
                    Task {
                        isSaving = true
                        defer { isSaving = false }
                        do {
                            try await save(modelName)
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
        .padding(24)
        .frame(width: 440)
    }
}
