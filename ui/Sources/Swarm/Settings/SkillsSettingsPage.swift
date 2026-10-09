import SwiftUI
import SwarmCore

struct SkillsSettingsPage: View {
    @Environment(\.designTokens) private var tokens
    @State private var model: SkillsCheckoutSettingsModel
    let onError: (String?) -> Void

    init(source: any SkillsCheckoutSettingsSource, onError: @escaping (String?) -> Void) {
        _model = State(initialValue: SkillsCheckoutSettingsModel(source: source))
        self.onError = onError
    }

    var body: some View {
        @Bindable var model = model
        ScrollView {
            VStack(alignment: .leading, spacing: tokens.spacing.m) {
                Text("Skills").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                Text("Swarm checkout").font(.headline)
                TextField("Swarm checkout", text: $model.path, prompt: Text("Absolute folder path"))
                    .textFieldStyle(.roundedBorder)
                    .disabled(model.saving)
                HStack {
                    Button("Choose folder", action: chooseFolder).disabled(model.saving)
                    Button("Save checkout") { Task { await model.save() } }.disabled(!model.canSave)
                    Button("Clear checkout") { Task { await model.clear() } }.disabled(!model.canClear)
                }
                if let path = model.savedPath {
                    Text(verbatim: path).textSelection(.enabled).font(.caption)
                }
                Text("Source edits stay in this checkout. Commit them so the next release bundles them.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .padding(tokens.spacing.xl)
        }
        .task { model.refresh() }
        .onChange(of: model.error, initial: true) { _, error in onError(error) }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose folder"
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            model.path = url.path
        }
    }
}

/// Removed when the source-setting owner lands in unit 3.
struct PendingSkillsCheckoutSettings: SkillsCheckoutSettingsSource {
    var checkoutPath: String? { nil }
    var loadError: String? { nil }

    func setCheckout(_ path: String?) async throws -> String? {
        throw PendingCheckoutError()
    }
}

private struct PendingCheckoutError: LocalizedError {
    var errorDescription: String? { "Checkout validation is not available yet." }
}
