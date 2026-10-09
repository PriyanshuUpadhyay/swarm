import SwiftUI
import SwarmCore

struct SkillsView: View {
    @Environment(\.designTokens) private var tokens
    @Environment(SettingsSelection.self) private var settings
    @Environment(\.openSettings) private var openSettings
    @State private var model = SkillsPageModel(source: SkillsFileSource())
    @State private var addWithSection = false
    @State private var confirmRemoval = false
    @State private var resolvingNavigation = false
    @State private var leaveAction: (() -> Void)?
    @FocusState private var focus: Field?
    let mode: String
    let keepOpen: () -> Void
    let leave: (String) -> Void

    private enum Field: Hashable { case name, node(String) }
    private struct CheckoutRequest: Equatable { let path: String?; let error: String? }
    private var isActive: Bool { mode == "Skills" }
    private var canDrawGraph: Bool { !model.steps.isEmpty && Set(model.steps.map(\.id)).count == model.steps.count }
    private var invalidSource: Bool { if case .invalidSource = model.document?.state { true } else { false } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if model.selectedKey == nil { list }
            else { editor }
        }
        .task(id: isActive) {
            guard isActive, model.entries.isEmpty else { return }
            _ = await settings.waitForSkillsRefresh()
            await model.loadList()
        }
        .task(id: CheckoutRequest(path: settings.prefs.skillsCheckout, error: settings.loadError)) {
            await model.changeCheckout(settings.prefs.skillsCheckout, loadError: settings.loadError)
        }
        .onChange(of: mode) { old, new in
            guard old == "Skills", new != "Skills", model.isDirty else { return }
            leaveAction = { leave(new) }
            keepOpen()
            Task { await model.requestLeave() }
        }
        .onKeyPress(.escape) {
            guard isActive, model.selectedKey != nil else { return .ignored }
            Task { await model.back() }
            return .handled
        }
        .background(SkillsWindowCloseGuard(dirty: model.isDirty) { close in
            leaveAction = close
            keepOpen()
            Task { await model.requestLeave() }
        })
        .confirmationDialog(model.pendingAction == .reload ? "Reload and discard your changes?" : "Save your changes before leaving this skill?",
                            isPresented: Binding(get: { model.pendingAction != nil }, set: { shown in
                                if !shown && !resolvingNavigation { Task { _ = await model.resolveNavigation(.cancel) } }
                            }), titleVisibility: .visible) {
            if model.pendingAction == .reload {
                Button("Reload", role: .destructive) { resolve(.discard) }
            } else {
                Button("Save") { resolve(.save) }.disabled(!model.canSave)
                Button("Discard", role: .destructive) { resolve(.discard) }
            }
            Button("Cancel", role: .cancel) { resolve(.cancel) }
        }
        .alert("Remove this step and its text?", isPresented: $confirmRemoval) {
            Button("Remove step", role: .destructive) { remove(confirmSection: true) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Save writes this removal to the checkout.")
        }
        .onChange(of: model.announcementRevision) { _, _ in
            if let text = model.error ?? model.notice ?? model.listError { AccessibilityNotification.Announcement(text).post() }
        }
        .onChange(of: model.document?.state) { _, state in
            if let reason = state?.reason { AccessibilityNotification.Announcement(reason).post() }
        }
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Skills").font(.headline).padding(tokens.spacing.m).accessibilityAddTraits(.isHeader)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: tokens.spacing.m) {
                    if model.loadingList { DelayedProgress("Reading skills…") }
                    if let error = model.listError {
                        Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
                        Button("Retry") { Task { await model.loadList() } }
                    }
                    ForEach(model.entries) { entry in
                        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                            Text(verbatim: entry.name).font(.subheadline.weight(.medium))
                            if let qualifier = entry.qualifier {
                                Text(verbatim: qualifier).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                            }
                            if entry.hasStepTable {
                                Button("Open graph") { Task { await model.open(key: entry.key) } }
                                    .accessibilityLabel("Open graph, \(entry.name), \(entry.qualifier ?? "")")
                            } else {
                                Text("no step table").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                    if model.entries.isEmpty && !model.loadingList && model.listError == nil {
                        Text("No bundled skills.").foregroundStyle(.secondary)
                    }
                    if let error = model.error {
                        Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
                        if model.canRetry { Button("Retry") { Task { await model.retry() } } }
                    }
                }
                .padding(tokens.spacing.m)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var editor: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Button("Skills", systemImage: "chevron.backward") { Task { await model.back() } }
                    .buttonStyle(.borderless).disabled(model.busy)
                Spacer()
                Text(verbatim: model.entries.first { $0.key == model.selectedKey }?.name ?? "Skill")
                    .font(.headline).accessibilityAddTraits(.isHeader)
            }.padding(tokens.spacing.m)
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: tokens.spacing.m) {
                    sourceNotice
                    if let error = model.error {
                        Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
                        if model.conflict && !model.canRetry { Button("Reload") { Task { await model.reload() } } }
                        else if model.canRetry { Button("Retry") { Task { await model.retry() } } }
                    }
                    if canDrawGraph {
                        graph
                        Divider()
                        inspector
                    } else if let document = model.document {
                        Text(verbatim: String(decoding: document.bytes, as: UTF8.self))
                            .font(.body.monospaced()).textSelection(.enabled)
                    }
                }
                .padding(tokens.spacing.m)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider()
            HStack {
                Button("Save") { Task { _ = await model.save() } }
                    .keyboardShortcut("s", modifiers: .command)
                    .disabled(!model.canSave || !isActive)
                Button("Discard") { model.discard() }.disabled(!model.isDirty || model.busy)
                if model.busy { DelayedProgress("Saving or reading…") }
            }.padding(tokens.spacing.m)
            if let notice = model.notice {
                Text(notice).font(.caption).padding([.horizontal, .bottom], tokens.spacing.m)
            }
        }
    }

    @ViewBuilder private var sourceNotice: some View {
        if let document = model.document {
            VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                switch document.state {
                case .checkoutEditable:
                    Text("Editing checkout").font(.caption.weight(.medium))
                    if let path = document.sourceURL?.path { Text(verbatim: path).font(.caption).textSelection(.enabled) }
                case .bundledReadOnly(let reason):
                    Text("Viewing bundled skill").font(.caption.weight(.medium))
                    Text(verbatim: reason).font(.caption).foregroundStyle(.secondary)
                    if document.key.hasPrefix("kit/skills/") { Button("Set checkout", action: setCheckout) }
                case .missingCheckoutSource(let reason):
                    Text("Viewing bundled skill").font(.caption.weight(.medium))
                    Text(verbatim: reason).font(.caption).foregroundStyle(.red).textSelection(.enabled)
                    Button("Retry") { Task { await model.retry() } }.disabled(model.busy)
                case .noTable:
                    Text("no step table").font(.caption).foregroundStyle(.secondary)
                case .invalidSource(let reason):
                    Text(verbatim: reason).font(.caption).foregroundStyle(.red).textSelection(.enabled)
                }
            }
        }
    }

    private var graph: some View {
        let layers = GraphLayers.layers(model.steps.map { (id: $0.id, needs: $0.needs) })
        let layerOf = Dictionary(uniqueKeysWithValues: layers.enumerated().flatMap { index, ids in ids.map { ($0, index) } })
        let edges = model.steps.flatMap { step in step.needs.compactMap { need -> GraphEdge? in
            guard let from = layerOf[need], let to = layerOf[step.id], from < to else { return nil }
            return GraphEdge(from: need, to: step.id)
        } }
        return ScrollView(.horizontal) {
            StepGraph(layers: layers, edges: edges) { id in
                if let step = model.steps.first(where: { $0.id == id }) {
                    GraphNodeButton(selected: model.selectedID == id,
                                    invalid: invalidSource,
                                    action: { model.select(id) }) {
                        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                            Text(verbatim: step.stem).font(.callout.weight(.medium))
                            if !step.holds.isEmpty { Text(verbatim: step.holds).font(.caption).foregroundStyle(.secondary).lineLimit(4) }
                        }
                    }
                    .frame(width: DesignTokens.Size.skillNode)
                    .focused($focus, equals: .node(id))
                    .disabled(model.busy)
                    .accessibilityLabel(nodeLabel(step))
                    .accessibilitySortPriority(Double(model.steps.count - (model.steps.firstIndex { $0.id == id } ?? 0)))
                    .accessibilityAddTraits(model.selectedID == id ? .isSelected : [])
                }
            }
        }
    }

    @ViewBuilder private var inspector: some View {
        if let step = model.selectedStep {
            VStack(alignment: .leading, spacing: tokens.spacing.s) {
                Text(verbatim: step.stem).font(.headline).accessibilityAddTraits(.isHeader)
                Text("Name").font(.caption)
                TextField("Name", text: Binding(get: { model.nameText }, set: model.setName))
                    .textFieldStyle(.roundedBorder).disabled(!model.canChangeStructure).focused($focus, equals: .name)
                Text("Needs").font(.caption)
                Toggle("None", isOn: Binding(get: { step.needs.isEmpty }, set: { if $0 { model.clearNeeds() } }))
                    .disabled(!model.canEdit)
                ForEach(model.needChoices) { need in
                    Toggle(need.stem, isOn: Binding(get: { model.selectedStep?.needs.contains(need.id) == true },
                                                  set: { model.setNeed(need.id, selected: $0) }))
                        .disabled(!model.canChooseNeed(need.id))
                        .help(model.canChooseNeed(need.id) ? "" : "This dependency cannot be selected. It can create a cycle or an ambiguous name.")
                }
                Text("Holds").font(.caption)
                TextField("Holds", text: Binding(get: { model.holdsText }, set: model.setHolds))
                    .textFieldStyle(.roundedBorder).disabled(!model.canEdit)
                Text("Step text").font(.caption)
                if step.body != nil {
                    Text(verbatim: "## \(step.stem)").font(.caption.monospaced()).foregroundStyle(.secondary)
                    TextEditor(text: Binding(get: { model.bodyText }, set: model.setBody))
                        .font(.body.monospaced()).frame(minHeight: DesignTokens.Size.skillText)
                        .disabled(!model.canEdit).accessibilityLabel("Step text")
                } else {
                    Text("no step section in SKILL.md").font(.caption).foregroundStyle(.secondary)
                }
                if let reason = model.structuralReason { Text(verbatim: reason).font(.caption).foregroundStyle(.secondary) }
                if let reason = model.removalReason, reason != model.structuralReason {
                    Text(verbatim: reason).font(.caption).foregroundStyle(.secondary)
                }
                Toggle("Include a section for the new step", isOn: $addWithSection).disabled(!model.canChangeStructure)
                Button("Add step") {
                    model.addStep(withSection: addWithSection)
                    focus = .name
                }.disabled(!model.canAdd)
                Button("Remove step") {
                    if step.body != nil { confirmRemoval = true }
                    else { remove(confirmSection: false) }
                }.disabled(!model.canRemove)
                HStack {
                    Button("Move up") { model.moveSelected(by: -1) }.disabled(!model.canMove(by: -1))
                    Button("Move down") { model.moveSelected(by: 1) }.disabled(!model.canMove(by: 1))
                }
            }.toggleStyle(.checkbox)
        }
    }

    private func nodeLabel(_ step: SkillDraftStep) -> String {
        let needs = step.needs.compactMap { id in model.steps.first { $0.id == id }?.stem }.joined(separator: ", ")
        var label = "\(step.stem), needs \(needs.isEmpty ? "none" : needs)"
        if model.selectedID == step.id {
            label += ", selected"
            if let error = model.error { label += ", \(error)" }
        }
        return label
    }

    private func setCheckout() {
        settings.select(.skills)
        openSettings()
    }

    private func remove(confirmSection: Bool) {
        model.removeSelected(confirmSection: confirmSection)
        focus = model.selectedID.map(Field.node) ?? .name
    }

    private func resolve(_ choice: SkillsNavigationChoice) {
        let action = model.pendingAction
        resolvingNavigation = true
        Task {
            let completed = await model.resolveNavigation(choice)
            if completed && action == .leave { leaveAction?() }
            leaveAction = nil
            resolvingNavigation = false
        }
    }
}
