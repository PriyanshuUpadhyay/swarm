import SwiftUI
import AppKit
import BloomCore

/// A Markdown or HTML file drawn as its document. See `DocumentPreview` for the rules and
/// `DocumentPreviewSession` for how the page stays current.
///
/// Refreshed on the workspace's change generation, which the worktree watcher moves when anything
/// in the worktree is written, and on the editor's unsaved text. Neither reloads a page that has
/// not changed: the session compares a fingerprint first.
struct DocumentPreviewView: View {
    /// Absolute.
    let path: String
    /// The worktree the file belongs to, or nil for a file opened from outside one.
    let worktree: String?
    let revision: Int
    var openFile: (String) -> Void = { _ in }

    @State private var session: DocumentPreviewSession?
    private let edits = FileEditSession.shared

    private struct RefreshKey: Equatable {
        var revision: Int
        var draft: String?
    }

    private var draft: String? {
        let draft = edits.draft(for: path)
        return draft?.isDirty == true ? draft?.text : nil
    }

    var body: some View {
        DocumentPreviewHost(session: session)
            .background(Palette.surface)
            .onAppear {
                guard session == nil else { return }
                let root = DocumentPreview.root(forFile: path, worktree: worktree)
                session = DocumentPreviewSession(document: path, root: root)
            }
            .onDisappear {
                session?.close()
                session = nil
            }
            .onChange(of: RefreshKey(revision: revision, draft: draft), initial: true) { _, key in
                session?.openFile = openFile
                session?.update(draft: key.draft)
            }
            .onChange(of: session == nil) { _, _ in
                session?.openFile = openFile
                session?.update(draft: draft)
            }
    }
}

private struct DocumentPreviewHost: NSViewRepresentable {
    let session: DocumentPreviewSession?

    func makeNSView(context: Context) -> BrowserHostView { BrowserHostView() }

    func updateNSView(_ view: BrowserHostView, context: Context) {
        if let session { view.attach(session.host) }
    }
}
