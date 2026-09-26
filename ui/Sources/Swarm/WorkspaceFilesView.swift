import SwiftUI
import SwarmCore

struct WorkspaceFilesView: View {
    let directory: String
    let open: (WorkspaceDocument) -> Void
    @State private var refreshID = 0

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Files").font(.headline)
                Spacer()
                Button { refreshID += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .buttonStyle(.plain).accessibilityLabel("Refresh files")
            }.padding(12)
            Divider()
            ScrollView {
                WorkspaceFolder(directory: directory, path: "", open: open)
                    .id(refreshID)
                    .frame(maxWidth: .infinity, alignment: .leading).padding(12)
            }
        }
    }
}

private struct WorkspaceFolder: View {
    let directory: String
    let path: String
    let open: (WorkspaceDocument) -> Void
    @State private var listing: WorkspaceFileListing?
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            if let error { Text(verbatim: error).foregroundStyle(.red) }
            else if let listing {
                ForEach(listing.entries) { entry in
                    WorkspaceFileRow(directory: directory, entry: entry, open: open)
                }
                if listing.entries.isEmpty { Text("Empty folder").foregroundStyle(.secondary) }
                if listing.truncated { Text("Showing 2,000 entries. This folder exceeds the list limit.").foregroundStyle(.secondary) }
            } else { ProgressView().controlSize(.small) }
        }
        .task {
            do {
                let value = try await WorkspaceFiles.list(in: directory, path: path)
                try Task.checkCancellation()
                listing = value
            } catch {
                if !Task.isCancelled { self.error = String(describing: error) }
            }
        }
    }
}

private struct WorkspaceFileRow: View {
    let directory: String
    let entry: WorkspaceFileEntry
    let open: (WorkspaceDocument) -> Void
    @State private var expanded = false

    var body: some View {
        if entry.kind == .directory {
            DisclosureGroup(isExpanded: $expanded) {
                if expanded { WorkspaceFolder(directory: directory, path: entry.path, open: open) }
            } label: {
                Label(entry.name, systemImage: "folder").lineLimit(1).help(entry.path)
            }
        } else {
            Button {
                open(WorkspaceDocument(title: entry.path, detail: "Read-only · \(directory)", isDiff: false) {
                    switch try await WorkspaceFiles.preview(in: directory, path: entry.path) {
                    case .text(let text), .notice(let text): return text
                    }
                })
            } label: {
                Label(entry.name, systemImage: entry.kind == .symbolicLink ? "link" : "doc")
                    .lineLimit(1).truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
            }
            .buttonStyle(.plain).padding(.vertical, 3).help(entry.path)
            .accessibilityLabel("Preview \(entry.path)")
        }
    }
}
