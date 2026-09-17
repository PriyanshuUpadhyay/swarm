import SwiftUI

/// Source and preview share one pane. The editing session keeps drafts when the source is hidden.
struct MarkdownPreviewContent<Content: View>: View {
    let path: String
    let worktree: String
    let revision: Int
    @Binding var isPresented: Bool
    var openFile: (String) -> Void = { _ in }
    @ViewBuilder var content: () -> Content

    var body: some View {
        if isPresented {
            DocumentPreviewView(path: path, worktree: worktree, revision: revision, openFile: openFile)
        } else {
            content()
        }
    }
}
