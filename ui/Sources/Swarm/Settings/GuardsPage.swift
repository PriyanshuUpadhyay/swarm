import SwiftUI
import SwarmCore

struct GuardsPage: View {
    @Environment(\.designTokens) private var tokens
    let load: () -> Result<GuardRules, GuardListError>
    let save: (inout GuardRulesEditor) throws -> Void
    let onError: (String?) -> Void
    let reportError: (String) -> Void
    @State private var editor = GuardRulesEditor()
    @State private var editing: UUID?

    var body: some View {
        GroupBox("Guards") {
            VStack(alignment: .leading, spacing: tokens.spacing.m) {
                HStack {
                    Button("Add") {
                        editor.add()
                        editing = editor.drafts.last?.id
                    }.disabled(!editor.canEdit)
                    Button("Reload") { reload(reportFailure: true) }
                    Spacer()
                    Button("Save") {
                        do {
                            try save(&editor)
                            editing = nil
                        } catch {
                            editor.recordSaveFailure(error)
                            if let message = editor.error { reportError(message) }
                        }
                    }.disabled(!editor.canSave)
                }
                if editor.drafts.isEmpty && editor.loadError == nil {
                    Text("No guard rules").foregroundStyle(.secondary)
                }
                ForEach($editor.drafts) { $draft in
                    VStack(alignment: .leading, spacing: tokens.spacing.s) {
                        HStack {
                            Text(draft.name).font(.headline)
                            Spacer()
                            Button(editing == draft.id ? "Finish editing" : "Edit") {
                                editing = editing == draft.id ? nil : draft.id
                            }
                            Button("Delete", role: .destructive) {
                                editor.delete(id: draft.id)
                                editing = nil
                            }.disabled(!editor.canEdit)
                        }
                        if editing == draft.id {
                            GuardFields(fields: $draft)
                        } else {
                            Text(verbatim: "Event: \(draft.event)")
                            Text(verbatim: "Tools: \(draft.allTools ? "All tools" : draft.tools.map(\.text).joined(separator: ", "))")
                            Text(verbatim: "Command: \(draft.command.map(\.text).joined(separator: " "))")
                            Text(verbatim: "Timeout: \(draft.timeout.isEmpty ? "3 (default)" : draft.timeout) \(draft.timeout == "1" ? "second" : "seconds")")
                        }
                        if let error = draft.error {
                            Text(error).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
                        }
                        Divider()
                    }
                    .textSelection(.enabled)
                }
                if !editor.canEdit {
                    Text("Guards cannot be edited until guards.json is fixed.")
                        .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(tokens.spacing.s)
        }
        .task { reload() }
        .onChange(of: editor.error, initial: true) { _, error in
            onError(error)
        }
    }

    private func reload(reportFailure: Bool = false) {
        editor.load(load())
        editing = nil
        if reportFailure, let error = editor.loadError, !error.isMissing, let message = editor.error {
            reportError(message)
        }
    }
}

private struct GuardFields: View {
    @Environment(\.designTokens) private var tokens
    @Binding var fields: GuardRuleFields

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.s) {
            TextField("Name", text: $fields.name)
            TextField("Event", text: $fields.event)
            Toggle("All tools", isOn: $fields.allTools)
            if !fields.allTools {
                ForEach($fields.tools) { $tool in
                    HStack {
                        TextField("Tool", text: $tool.text)
                        Button("Remove tool") { fields.tools.removeAll { $0.id == tool.id } }
                            .accessibilityLabel("Remove tool \(tool.text)")
                    }
                }
                Button("Add tool") { fields.tools.append(GuardField(text: "")) }
            }
            ForEach($fields.command) { $argument in
                HStack {
                    TextField(fields.command.first?.id == argument.id ? "Command" : "Argument", text: $argument.text)
                    Button("Remove argument") { fields.command.removeAll { $0.id == argument.id } }
                        .accessibilityLabel("Remove argument \(argument.text)")
                }
            }
            Button("Add argument") { fields.command.append(GuardField(text: "")) }
            Text("Each field is one command argument. A path with spaces stays in one field.")
                .font(.caption).foregroundStyle(.secondary)
            TextField("Timeout in seconds (empty uses 3)", text: $fields.timeout)
        }
        .textFieldStyle(.roundedBorder)
    }
}
