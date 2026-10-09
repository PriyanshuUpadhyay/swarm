import SwiftUI
import SwarmCore

struct GuardsPage: View {
    let load: () -> Result<GuardRules, GuardListError>
    let save: (inout GuardRulesEditor) throws -> Void
    let onError: (String?) -> Void
    @State private var editor = GuardRulesEditor()
    @State private var editing: Int?

    var body: some View {
        GroupBox("Guards") {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                HStack {
                    Button("Add") {
                        editor.add()
                        editing = editor.drafts.indices.last
                    }.disabled(editor.loadError != nil)
                    Button("Reload", action: reload)
                    Spacer()
                    Button("Save") {
                        do {
                            try save(&editor)
                            editing = nil
                            onError(nil)
                        } catch { onError(error.localizedDescription) }
                    }.disabled(!editor.canSave)
                }
                if editor.drafts.isEmpty && editor.loadError == nil {
                    Text("No guard rules").foregroundStyle(.secondary)
                }
                ForEach(editor.drafts.indices, id: \.self) { index in
                    VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
                        HStack {
                            Text(editor.drafts[index].name).font(.headline)
                            Spacer()
                            Button(editing == index ? "Finish editing" : "Edit") {
                                editing = editing == index ? nil : index
                            }
                            Button("Delete", role: .destructive) {
                                editor.delete(at: index)
                                editing = nil
                            }
                        }
                        if editing == index {
                            GuardFields(fields: $editor.drafts[index])
                        } else {
                            Text(verbatim: "Event: \(editor.drafts[index].event)")
                            Text(verbatim: "Tools: \(editor.drafts[index].tools?.joined(separator: ", ") ?? "All tools")")
                            Text(verbatim: "Command: \(editor.drafts[index].command.joined(separator: " "))")
                            Text(verbatim: "Timeout: \(editor.drafts[index].timeout.isEmpty ? "3 (default)" : editor.drafts[index].timeout) seconds")
                        }
                        Divider()
                    }
                    .textSelection(.enabled)
                }
                if editor.loadError == nil, let error = editor.error {
                    Text(error).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(DesignTokens.Spacing.s)
        }
        .task { reload() }
    }

    private func reload() {
        editor.load(load())
        editing = nil
        onError(editor.loadError.map {
            "guards.json could not be read: \($0.reason). Every tool call is blocked until it is fixed."
        })
    }
}

private struct GuardFields: View {
    @Binding var fields: GuardRuleFields

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.Spacing.s) {
            TextField("Name", text: $fields.name)
            TextField("Event", text: $fields.event)
            Toggle("All tools", isOn: Binding(
                get: { fields.tools == nil },
                set: { fields.tools = $0 ? nil : [""] }
            ))
            if let tools = fields.tools {
                ForEach(tools.indices, id: \.self) { index in
                    HStack {
                        TextField("Tool", text: Binding(
                            get: { fields.tools?[index] ?? "" },
                            set: { fields.tools?[index] = $0 }
                        ))
                        Button("Remove tool") { fields.tools?.remove(at: index) }
                    }
                }
                Button("Add tool") { fields.tools?.append("") }
            }
            ForEach(fields.command.indices, id: \.self) { index in
                HStack {
                    TextField(index == 0 ? "Command" : "Argument", text: $fields.command[index])
                    Button("Remove argument") { fields.command.remove(at: index) }
                }
            }
            Button("Add argument") { fields.command.append("") }
            Text("Each field is one command argument. A path with spaces stays in one field.")
                .font(.caption).foregroundStyle(.secondary)
            TextField("Timeout in seconds (empty uses 3)", text: $fields.timeout)
        }
        .textFieldStyle(.roundedBorder)
    }
}
