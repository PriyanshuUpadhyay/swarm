import SwiftUI
import SwarmCore

struct NewWorkspaceSheet: View {
    let projectName: String
    let projectPath: String
    let loadReferences: () async throws -> WorkspaceReferences
    let create: (WorkspaceRequest, ProjectDefaults) async throws -> String
    let onCreated: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var mode = StartMode.newBranch
    @State private var base = ""
    @State private var existingBranch = ""
    @State private var pullRequest = ""
    @State private var worktreeFolder: String
    @State private var branchPrefix: String
    @State private var references: WorkspaceReferences?
    @State private var isCreating = false
    @State private var error: String?

    private enum StartMode { case newBranch, existingBranch, pullRequest }

    init(
        projectName: String, projectPath: String, worktreeFolder: String, branchPrefix: String,
        loadReferences: @escaping () async throws -> WorkspaceReferences,
        create: @escaping (WorkspaceRequest, ProjectDefaults) async throws -> String,
        onCreated: @escaping (String) -> Void
    ) {
        self.projectName = projectName
        self.projectPath = projectPath
        self.loadReferences = loadReferences
        self.create = create
        self.onCreated = onCreated
        _worktreeFolder = State(initialValue: worktreeFolder)
        _branchPrefix = State(initialValue: branchPrefix)
    }

    private var request: WorkspaceRequest {
        let start: WorkspaceStart
        switch mode {
        case .newBranch: start = .newBranch(base: base)
        case .existingBranch: start = .existingBranch(existingBranch)
        case .pullRequest: start = .pullRequest(Int(pullRequest.trimmingCharacters(in: .whitespacesAndNewlines)) ?? 0)
        }
        return WorkspaceRequest(name: name, start: start, prefix: branchPrefix)
    }

    private var canCreate: Bool {
        guard let references else { return false }
        return !isCreating && NewWorkspaceForm.canCreate(request, references: references)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.l) {
            Text("New workspace in \(projectName)").font(.title2)
            Text(verbatim: projectPath).foregroundStyle(.secondary).lineLimit(2)
            Grid(alignment: .leading, horizontalSpacing: DesignTokens.Spacing.m, verticalSpacing: DesignTokens.Spacing.m) {
                GridRow {
                    Text("Name")
                    TextField("Workspace name", text: $name)
                        .accessibilityLabel("Workspace name")
                }
                GridRow {
                    Text("Start from").gridCellAnchor(.topLeading)
                    startFields
                }
                GridRow {
                    Text("Branch")
                    HStack(spacing: DesignTokens.Spacing.xs) {
                        Text(verbatim: NewWorkspaceForm.preview(request) ?? "—")
                            .font(DesignTokens.mono).textSelection(.enabled).lineLimit(2)
                        if mode == .existingBranch { Text("(as given)").foregroundStyle(.secondary) }
                    }
                }
            }
            .disabled(isCreating)
            DisclosureGroup("Project defaults") {
                Grid(alignment: .leading, horizontalSpacing: DesignTokens.Spacing.m, verticalSpacing: DesignTokens.Spacing.m) {
                    GridRow {
                        Text("Worktree folder")
                        TextField("Worktree folder", text: $worktreeFolder)
                    }
                    GridRow {
                        Text("Branch prefix")
                        TextField("Branch prefix", text: $branchPrefix)
                    }
                }
                .padding(.top, DesignTokens.Spacing.s)
            }
            .disabled(isCreating)
            if let error { Text(verbatim: error).foregroundStyle(.red) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(isCreating)
                Button(isCreating ? "Creating…" : "Create", action: createWorkspace)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canCreate)
            }
        }
        .padding(DesignTokens.Spacing.xl)
        .frame(width: DesignTokens.Size.sheet)
        .interactiveDismissDisabled(isCreating)
        .task {
            do {
                let loaded = try await loadReferences()
                base = loaded.defaultBranch == nil ? "" : loaded.bases.first ?? ""
                existingBranch = (loaded.local + loaded.remote).first ?? ""
                references = loaded
            } catch {
                self.error = "Could not load branches. \(error.localizedDescription)"
            }
        }
    }

    @ViewBuilder private var startFields: some View {
        if let references {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                HStack(spacing: DesignTokens.Spacing.s) {
                    modeButton("New branch on", mode: .newBranch)
                    if references.defaultBranch != nil {
                        Picker("Base branch", selection: $base) {
                            ForEach(references.bases, id: \.self) { Text(verbatim: $0).tag($0) }
                        }
                        .labelsHidden()
                        .disabled(mode != .newBranch)
                    }
                }
                if references.defaultBranch == nil {
                    Text("The first workspace starts an orphan branch (no commit yet)")
                        .font(.callout).foregroundStyle(.secondary)
                } else {
                    HStack(spacing: DesignTokens.Spacing.s) {
                        modeButton("Existing branch", mode: .existingBranch)
                        Picker("Existing branch", selection: $existingBranch) {
                            ForEach(references.local + references.remote, id: \.self) { Text(verbatim: $0).tag($0) }
                        }
                        .labelsHidden()
                        .disabled(mode != .existingBranch || (references.local + references.remote).isEmpty)
                    }
                    HStack(spacing: DesignTokens.Spacing.s) {
                        modeButton("Pull request", mode: .pullRequest)
                        Text("#")
                        TextField("42", text: $pullRequest)
                            .accessibilityLabel("Pull request number")
                            .disabled(mode != .pullRequest)
                    }
                }
            }
        } else if error == nil {
            ProgressView("Loading branches…")
        }
    }

    private func modeButton(_ title: String, mode choice: StartMode) -> some View {
        Button { mode = choice } label: {
            HStack(spacing: DesignTokens.Spacing.xs) {
                Image(systemName: mode == choice ? "largecircle.fill.circle" : "circle")
                Text(title)
            }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(mode == choice ? .isSelected : [])
    }

    private func createWorkspace() {
        guard canCreate else { return }
        let request = request
        let defaults = ProjectDefaults(worktreeFolder: worktreeFolder, branchPrefix: branchPrefix)
        isCreating = true
        error = nil
        Task {
            defer { isCreating = false }
            do {
                onCreated(try await create(request, defaults))
                dismiss()
            } catch {
                self.error = "Could not add the workspace. \(error.localizedDescription)"
            }
        }
    }
}
