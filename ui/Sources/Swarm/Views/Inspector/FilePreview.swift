import SwiftUI
import SwarmCore

/// Reading and editing share the native text surface, including selection, find and navigation.
struct FilePreview: View {
    let model: WorkspaceModel
    let path: String
    var absolutePathOverride: String?
    var canEditInSwarm = true
    @State private var width: CGFloat = 0
    private let session = FileEditSession.shared

    private var absolutePath: String {
        absolutePathOverride ?? (model.workspace.path as NSString).appendingPathComponent(path)
    }
    private var state: SourceEditorState { SourceEditorState.file(absolutePath) }
    private var hasPreview: Bool { DocumentPreview.kind(path: path) != nil }
    private var modes: [FileTabMode] { FileTabMode.choices(hasPreview: hasPreview, canEdit: canEditInSwarm) }
    private var mode: FileTabMode {
        FileTabMode.current(
            prefersEditing: state.prefersEditing, prefersPreview: state.prefersPreview,
            hasPreview: hasPreview, canEdit: canEditInSwarm
        )
    }

    private func choose(_ mode: FileTabMode) {
        let preferences = mode.preferences(prefersPreview: state.prefersPreview)
        state.prefersEditing = preferences.prefersEditing
        state.prefersPreview = preferences.prefersPreview
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: InspectorLayout.gap) {
                FilePathLabel(path: path, width: width)
                UnsavedEditsDot(session: session, path: absolutePath)
                Spacer(minLength: InspectorLayout.tight)
                Menu {
                    OpenInAppItems(target: .file(absolutePath))
                } label: {
                    Image(systemName: "arrow.up.forward.app")
                } primaryAction: {
                    Reveal.inEditor(absolutePath, repo: model.repo?.id)
                }
                .menuStyle(.borderlessButton).menuIndicator(.hidden)
                .help("Open in your editor")
                if modes.count > 1 {
                    Picker("File view", selection: Binding(
                        get: { mode }, set: { choose($0) }
                    )) {
                        ForEach(modes, id: \.self) { mode in
                            Text(mode.title(hasPreview: hasPreview)).tag(mode)
                        }
                    }.pickerStyle(.segmented).labelsHidden().fixedSize()
                }
            }
            .controlSize(.small)
            .padding(.horizontal, InspectorLayout.inset)
            .frame(height: InspectorLayout.barHeight)
            .background(Palette.surfaceSunken)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            Hairline()
            if mode == .preview {
                DocumentPreviewView(
                    path: absolutePath,
                    worktree: absolutePathOverride == nil ? model.workspace.path : nil,
                    revision: model.changesGeneration,
                    openFile: { FileReview.open(absolutePath: $0, in: model) }
                )
            } else {
                FileEditPane(model: model, path: path, session: session,
                             isEditable: mode == .edit, absolutePathOverride: absolutePathOverride)
            }
        }
        .background(Palette.surface)
        .environment(\.openInRepoID, model.repo?.id)
        .onAppear { if session.isDirty(absolutePath) { state.prefersEditing = true } }
        .background {
            Button("Toggle View and Edit") { state.prefersEditing.toggle() }
                .keyboardShortcut("e", modifiers: .command)
                .disabled(!canEditInSwarm)
                .frame(width: 0, height: 0).opacity(0).accessibilityHidden(true)
        }
        .background {
            // VS Code's key for the same thing.
            Button("Toggle Preview and Source") { choose(mode == .preview ? .source : .preview) }
                .keyboardShortcut("v", modifiers: [.command, .shift])
                .disabled(!hasPreview)
                .frame(width: 0, height: 0).opacity(0).accessibilityHidden(true)
        }
    }
}
