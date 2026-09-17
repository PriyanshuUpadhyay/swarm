import SwiftUI
import SwarmCore

/// Settings, Model Presets: the list the composer's model menu shows, in its order, with the one
/// new sessions start on.
///
/// Presets are made two ways and both land here: "Save as preset…" in the composer's settings
/// panel saves what a chat is set to, and "Add preset…" below builds one from nothing. Every edit
/// goes through `ModelPresetLibrary.shared`, which is the same copy the composer reads, so a
/// rename here is in the menu the next time it opens.
struct ModelPresetSettingsView: View {
    @Environment(AppModel.self) private var app

    @State private var editing: ModelPresetDraft?
    @State private var renaming: ModelPreset?
    @State private var renameText = ""

    private var library: ModelPresetLibrary { .shared }
    private var catalog: ComposerModelCatalog { .shared }

    var body: some View {
        Form {
            Section {
                if library.presets.isEmpty {
                    Text("No presets yet. Add one here, or choose Save as preset… in a chat's agent settings.")
                        .settingsFootnote()
                } else {
                    ForEach(library.presets) { preset in
                        row(preset)
                            .contextMenu { actions(for: preset) }
                    }
                    .onMove { source, destination in
                        library.move(fromOffsets: source, toOffset: destination, in: app.store)
                    }
                }

                Button("Add preset…") { editing = ModelPresetDraft(preset: newPreset()) }
            } header: {
                Text("Presets")
            } footer: {
                Text("Shown in the model menu, in this order.")
                    .settingsFootnote()
            }

            Section {
                Picker("Default for new sessions", selection: defaultBinding) {
                    Text("None").tag(ModelPresetID?.none)
                    ForEach(library.presets) { preset in
                        Text(preset.name).tag(ModelPresetID?.some(preset.id))
                    }
                }
            } header: {
                Text("New sessions")
            } footer: {
                Text("With a default preset, new sessions start on its model, reasoning, output style and permissions instead of the choices under Sessions. Project model settings still take priority.")
                    .settingsFootnote()
            }
        }
        .settingsForm()
        .task {
            catalog.load()
            await library.load(from: app.store)
        }
        .sheet(item: $editing) { draft in
            ModelPresetEditor(draft: draft) { saved in
                if library.list.preset(id: saved.id) == nil {
                    library.add(saved, in: app.store)
                } else {
                    library.update(saved, in: app.store)
                }
                editing = nil
            } onCancel: {
                editing = nil
            }
        }
        .alert("Rename preset", isPresented: isRenaming) {
            TextField("Name", text: $renameText)
            Button("Rename") {
                if let renaming { library.rename(id: renaming.id, to: renameText, in: app.store) }
                renaming = nil
            }
            .disabled(ModelPreset.cleanName(renameText) == nil)
            Button("Cancel", role: .cancel) { renaming = nil }
        }
    }

    private func row(_ preset: ModelPreset) -> some View {
        HStack(spacing: Metrics.gutter) {
            Image(systemName: "line.3.horizontal")
                .foregroundStyle(Palette.textTertiary)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: Metrics.spacingTight) {
                HStack(spacing: Metrics.spacing) {
                    Text(preset.name)
                        .lineLimit(1)
                    if library.list.defaultID == preset.id {
                        Text("Default")
                            .font(Typo.caption)
                            .foregroundStyle(Palette.accent)
                            .padding(.horizontal, Metrics.spacingSmall)
                            .overlay {
                                RoundedRectangle(cornerRadius: Metrics.cornerSmall)
                                    .strokeBorder(Palette.accent.opacity(0.4), lineWidth: Metrics.outline)
                            }
                    }
                }
                Text(summary(of: preset))
                    .font(Typo.caption)
                    .foregroundStyle(Palette.textSecondary)
                    .lineLimit(1)
            }

            Spacer(minLength: Metrics.spacing)

            Menu {
                actions(for: preset)
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Preset actions")
            .accessibilityLabel("Actions for \(preset.name)")
        }
        .padding(.vertical, Metrics.spacingTight)
    }

    @ViewBuilder
    private func actions(for preset: ModelPreset) -> some View {
        Button("Edit…") { editing = ModelPresetDraft(preset: preset) }
        Button("Rename…") {
            renameText = preset.name
            renaming = preset
        }
        if library.list.defaultID == preset.id {
            Button("Stop using for new sessions") { library.setDefault(nil, in: app.store) }
        } else {
            Button("Use for new sessions") { library.setDefault(preset.id, in: app.store) }
        }
        Divider()
        Button("Move up") { library.move(id: preset.id, by: -1, in: app.store) }
            .disabled(library.presets.first?.id == preset.id)
        Button("Move down") { library.move(id: preset.id, by: 1, in: app.store) }
            .disabled(library.presets.last?.id == preset.id)
        Divider()
        Button("Delete", role: .destructive) { library.delete(id: preset.id, in: app.store) }
    }

    /// Model, reasoning and permissions, in the words the composer uses for them.
    private func summary(of preset: ModelPreset) -> String {
        let model = ComposerOption.label(
            for: preset.model,
            in: catalog.sections(includingCurrent: preset.model, on: preset.backend).flatMap(\.options)
        )
        let effort = ComposerOption.label(
            for: preset.effort, in: catalog.efforts(for: preset.backend, model: preset.model)
        )
        return "\(model) · \(effort) · \(preset.permissionMode.label(on: preset.backend))"
    }

    /// A new preset starts on the built-in fallbacks, which the editor is there to change.
    private func newPreset() -> ModelPreset {
        ModelPreset(
            name: "",
            model: AppDefaults.fallbackModel,
            effort: AppDefaults.fallbackEffort,
            backend: AppDefaults.fallbackBackend,
            permissionMode: AppDefaults.fallbackPermissionMode
        )
    }

    private var defaultBinding: Binding<ModelPresetID?> {
        Binding(
            get: { library.list.defaultID },
            set: { id in MainActor.assumeIsolated { library.setDefault(id, in: app.store) } }
        )
    }

    private var isRenaming: Binding<Bool> {
        Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
    }
}
