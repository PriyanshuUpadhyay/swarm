import SwiftUI
import SwarmCore

struct WorkspaceFilesView: View {
    @Environment(\.designTokens) private var tokens
    let directory: String
    var isActive = true
    let open: (WorkspaceDocument) -> Void
    @State private var refreshID = 0
    @State private var refreshing = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Files").font(.headline)
                Spacer()
                if refreshing { DelayedProgress().accessibilityLabel("Refreshing files") }
                Button { refreshing = true; refreshID += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .buttonStyle(.plain).accessibilityLabel("Refresh files")
                    .disabled(refreshing || !isActive)
            }.padding(tokens.spacing.m)
            Divider()
            ScrollView {
                WorkspaceFolder(directory: directory, path: "", refreshID: refreshID, isActive: isActive, open: open) {
                    refreshing = false
                }
                    .frame(maxWidth: .infinity, alignment: .leading).padding(tokens.spacing.m)
            }
        }
    }
}

private struct WorkspaceFolder: View {
    @Environment(\.designTokens) private var tokens
    let directory: String
    let path: String
    let refreshID: Int
    let isActive: Bool
    let open: (WorkspaceDocument) -> Void
    var onFinish: (@MainActor () -> Void)? = nil
    @State private var listing: WorkspaceFileListing?
    @State private var listingDirectory: String?
    @State private var error: String?
    @State private var errorDirectory: String?
    @State private var loading = true

    private struct Request: Equatable {
        let directory: String
        let path: String
        let refreshID: Int
        let isActive: Bool
    }

    var body: some View {
        LazyVStack(alignment: .leading, spacing: tokens.spacing.xs) {
            if loading || (listingDirectory != directory && errorDirectory != directory) {
                DelayedProgress("Reading files…")
            }
            if let error, errorDirectory == directory {
                Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled)
            }
            if let listing, listingDirectory == directory {
                if error != nil && errorDirectory == directory {
                    Text("Showing the last file list.").font(.caption).foregroundStyle(.secondary)
                }
                ForEach(listing.entries) { entry in
                    WorkspaceFileRow(directory: directory, entry: entry, refreshID: refreshID, isActive: isActive, open: open)
                }
                if listing.entries.isEmpty { Text("Empty folder").foregroundStyle(.secondary) }
                if listing.truncated { Text("Showing 2,000 entries. This folder exceeds the list limit.").foregroundStyle(.secondary) }
            } else if !loading && errorDirectory == directory && error == nil {
                Text("No files to show.").foregroundStyle(.secondary)
            }
        }
        .task(id: Request(directory: directory, path: path, refreshID: refreshID, isActive: isActive)) {
            guard isActive else { return }
            loading = true
            error = nil
            do {
                let value = try await WorkspaceFiles.list(in: directory, path: path)
                try Task.checkCancellation()
                listing = value
                listingDirectory = directory
            } catch {
                if !Task.isCancelled {
                    self.error = String(describing: error)
                    errorDirectory = directory
                }
            }
            guard !Task.isCancelled else { return }
            loading = false
            onFinish?()
        }
    }
}

private struct WorkspaceFileRow: View {
    @Environment(\.designTokens) private var tokens
    let directory: String
    let entry: WorkspaceFileEntry
    let refreshID: Int
    let isActive: Bool
    let open: (WorkspaceDocument) -> Void
    @State private var expanded = false

    var body: some View {
        if entry.kind == .directory {
            DisclosureGroup(isExpanded: $expanded) {
                if expanded {
                    WorkspaceFolder(directory: directory, path: entry.path, refreshID: refreshID, isActive: isActive, open: open)
                }
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
            .buttonStyle(.plain).padding(.vertical, tokens.spacing.xxs).help(entry.path)
            .accessibilityLabel("Preview \(entry.path)")
        }
    }
}
