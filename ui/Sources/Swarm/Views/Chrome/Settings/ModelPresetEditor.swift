import SwiftUI
import SwarmCore

/// A preset being edited in the sheet. Its own identity so `sheet(item:)` opens afresh for every
/// press, including a second press of Add.
struct ModelPresetDraft: Identifiable {
    let id = UUID()
    var preset: ModelPreset
}

/// Name, model and reasoning, output style and permissions, for one preset.
struct ModelPresetEditor: View {
    var onSave: (ModelPreset) -> Void
    var onCancel: () -> Void

    @State private var preset: ModelPreset
    @State private var outputStyles = ComposerOutputStyleCatalog()

    init(
        draft: ModelPresetDraft,
        onSave: @escaping (ModelPreset) -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.onSave = onSave
        self.onCancel = onCancel
        _preset = State(initialValue: draft.preset)
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                TextField("Name", text: $preset.name, prompt: Text("Opus 5 High"))

                SettingsRow("Model") {
                    ModelAndEffortPickers(
                        model: $preset.model, effort: $preset.effort, backend: backendBinding
                    )
                }

                if preset.backend == .claudeCode {
                    Picker("Output style", selection: $preset.outputStyle) {
                        ForEach(outputStyles.options(includingCurrent: preset.outputStyle)) { option in
                            Text(option.label).tag(option.id)
                        }
                    }
                }

                Picker("Permissions", selection: $preset.permissionMode) {
                    ForEach(ComposerControls(agentKind: preset.backend).permissionModeChoices) { choice in
                        Text(choice.label).tag(choice.mode)
                    }
                }
            }
            .settingsForm()

            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    guard let name = ModelPreset.cleanName(preset.name) else { return }
                    var saved = preset
                    saved.name = name
                    onSave(saved)
                }
                .keyboardShortcut(.defaultAction)
                .disabled(ModelPreset.cleanName(preset.name) == nil)
            }
            .padding(Metrics.gutter)
        }
        .frame(width: 480, height: 300)
        .task { await outputStyles.refreshIfStale(project: nil) }
    }

    /// Moving the backend moves the permission mode onto one that backend has, which is the rule
    /// `ComposerControls.agentKind` holds and a preset has to hold too.
    private var backendBinding: Binding<AgentKind> {
        Binding(
            get: { preset.backend },
            set: { kind in
                preset.backend = kind
                preset.permissionMode = preset.permissionMode.nearest(on: kind)
            }
        )
    }
}
