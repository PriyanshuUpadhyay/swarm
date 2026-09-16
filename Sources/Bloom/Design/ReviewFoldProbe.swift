import AppKit
import BloomCore
import SwiftUI

#if DEBUG
/// Checks that ticking a file in the all-files review folds it, in an invisible window against a
/// disposable repository.
///
/// The reader says "done with this one" and the diff gets out of the way; taking the tick off
/// brings it back. Measured by the height of the review's document, because a read file keeping
/// its six hundred points of scrolling is the complaint, and by four captures for the eye.
///
/// **The resize between the ticks is the part worth keeping.** A rule of "folded if viewed" would
/// also pass every check above it and then slam a hand opened file shut on the next diff stat
/// poll, so the probe re-lays the whole review out twice with nothing about the ticks moving and
/// insists nothing about the folding moved either. The hand opened case itself is
/// `ReviewCollapseTests`, over the rule this view calls.
///
/// **What is measured and what is asked.** The first tick is measured, because taking six hundred
/// points out of the document is the thing the reader sees and every machine agrees about it. The
/// second tick and the resize are asked of the view through `ReviewFoldReport`, because the review
/// is a lazy stack: its document height is an estimate over the sections nothing has laid out, and
/// on the CI runner a resize re-estimated it from 2,760 points to 300 with no tick having moved.
/// This probe passed here and failed there twice before that was believed.
///
/// The last capture is taken with every file ticked, so all four header rows stack and the row
/// controls can be read down one column: the preview button is drawn for the two Markdown files
/// and for neither of the others, and it leads the cluster so the tick and the overflow menu are
/// in the same place on all four.
@MainActor
enum ReviewFoldProbe {
    static func run(directory: String, check: (Bool, String) -> Void) async {
        // The store lives in the throwaway probe root, named before anything opens it. Without it
        // the model has no store, `setViewed` writes nowhere, and the probe would pass by
        // measuring nothing.
        setenv("BLOOM_DB_PATH", directory + "/fold.sqlite", 1)
        let app = AppModel()
        await app.bootstrap()
        guard let store = app.store else {
            check(false, "the fold probe could not open its own database")
            return
        }

        let model: WorkspaceModel
        do {
            model = try await seed(directory: directory, app: app, store: store)
        } catch {
            check(false, "fold fixture failed: \(error)")
            return
        }
        await model.refreshChanges()
        await model.reloadViewedFiles()
        check(model.reviewFiles.count == 4, "fold fixture loaded \(model.reviewFiles.count) files, expected four")
        let previewable = model.reviewFiles.filter { $0.path.hasSuffix(".md") }
        check(previewable.count == 2 && model.reviewFiles.count - previewable.count == 2,
              "fold fixture is not the mix of previewable and plain files the capture needs")
        guard let readme = model.reviewFiles.first(where: { $0.path == "README.md" }),
              let checkout = model.reviewFiles.first(where: { $0.path == "Sources/Checkout.swift" }) else {
            check(false, "fold fixture is missing the files the checks are about")
            return
        }

        model.selectedFilePath = readme.path
        let tab = CenterTabStore.shared.showReview(path: readme.path, workspaceID: model.workspace.id)
        CenterTabStore.shared.setShowsAllFiles(true, for: tab)
        let host = NSHostingView(rootView: Fixture(model: model))
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 1120, height: 760),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = host
        await settle(window)
        save(host, at: directory + "/fold-expanded.png")
        let expanded = documentHeight(in: host)
        check(expanded > 0, "the review drew no document to measure")

        await model.setViewed(true, file: readme)
        await settle(window)
        check(model.isViewed(readme), "the tick did not reach the model, so nothing below means anything")
        save(host, at: directory + "/fold-viewed.png")
        let folded = documentHeight(in: host)
        check(folded < expanded - 40,
              "ticking a file left the document at \(folded), from \(expanded): the diff did not fold")

        // A neighbour's tick folds its own file and leaves the one already folded alone.
        //
        // **Asked of the view rather than measured off it, unlike the check above.** The first tick
        // takes six hundred points out of the document and every machine agrees about that. The
        // second takes out a section that a lazy stack may never have laid out, so what the height
        // does depends on what has been materialised: on the CI runner it did not move at all. What
        // the check is actually about is which files are folded, and the view says so.
        await model.setViewed(true, file: checkout)
        await settle(window)
        check(ReviewFoldReport.collapsed.contains(checkout.path),
              "ticking a second file did not fold it as well: \(ReviewFoldReport.collapsed)")
        check(ReviewFoldReport.collapsed.contains(readme.path),
              "ticking a second file unfolded the first: \(ReviewFoldReport.collapsed)")

        // The re-render case: nothing about the ticks moves, so nothing about the folding may.
        let foldedPaths = ReviewFoldReport.collapsed
        window.setContentSize(NSSize(width: 980, height: 760))
        await settle(window)
        window.setContentSize(NSSize(width: 1120, height: 760))
        await settle(window)
        check(ReviewFoldReport.collapsed == foldedPaths,
              "a resize moved the folding: \(ReviewFoldReport.collapsed), from \(foldedPaths)")

        for file in model.reviewFiles { await model.setViewed(true, file: file) }
        await settle(window)
        save(host, at: directory + "/fold-all-viewed.png")
        let allFolded = documentHeight(in: host)
        check(allFolded < InspectorLayout.reviewHeaderHeight * CGFloat(model.reviewFiles.count) + 4,
              "four ticked files came to \(allFolded), which is more than four header rows")

        for file in model.reviewFiles { await model.setViewed(false, file: file) }
        await settle(window)
        save(host, at: directory + "/fold-unviewed.png")
        let reopened = documentHeight(in: host)
        // Against the folded height rather than against `expanded`. The stack is lazy, so the
        // height of a section nothing has scrolled to is an estimate, and an estimate that was
        // once measured and then folded away does not come back the same number. What is being
        // asked here is whether the diffs came back at all.
        check(reopened > allFolded + 400,
              "taking every tick off left the document at \(reopened), barely over the folded \(allFolded)")

        check(!window.isVisible && !window.isKeyWindow, "fold probe activated its window")
        window.contentView = nil
        withExtendedLifetime(app) {}
    }

    private static func documentHeight(in view: NSView) -> CGFloat {
        scrollView(in: view)?.documentView?.bounds.height ?? 0
    }

    private static func scrollView(in view: NSView) -> NSScrollView? {
        if let scroll = view as? NSScrollView { return scroll }
        return view.subviews.lazy.compactMap { scrollView(in: $0) }.first
    }

    private static func save(_ host: NSView, at path: String) {
        guard let bitmap = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return }
        host.cacheDisplay(in: host.bounds, to: bitmap)
        guard let data = bitmap.representation(using: .png, properties: [:]) else { return }
        try? data.write(to: URL(fileURLWithPath: path))
    }

    private static func settle(_ window: NSWindow) async {
        for _ in 0..<30 {
            window.contentView?.layoutSubtreeIfNeeded()
            try? await Task.sleep(for: .milliseconds(30))
        }
    }

    /// A real worktree with rows in the store behind it. The rows are the part that is easy to
    /// skip and cannot be: `reviewed_files` references `workspaces(id)`, so a tick against a
    /// workspace nobody wrote is refused by SQLite and the model quietly keeps its old answer.
    ///
    /// Two files something can be previewed from and two that cannot, which is the mix the
    /// capture needs and the shape of any real branch.
    private static func seed(directory: String, app: AppModel, store: Store) async throws -> WorkspaceModel {
        let origin = directory + "/fold-repo"
        let worktree = directory + "/fold"
        try FileManager.default.createDirectory(atPath: origin, withIntermediateDirectories: true)
        func git(_ arguments: [String], cwd: String = origin) async throws {
            try await Shell.check("git", ["-c", "commit.gpgsign=false", "-c", "user.name=Fold Probe",
                                          "-c", "user.email=fold@example.test"] + arguments, cwd: cwd)
        }
        func write(_ name: String, _ body: String, in root: String) throws {
            let path = root + "/" + name
            try FileManager.default.createDirectory(
                atPath: (path as NSString).deletingLastPathComponent, withIntermediateDirectories: true
            )
            try body.write(toFile: path, atomically: true, encoding: .utf8)
        }
        try await git(["init", "-q", "-b", "main"])
        try write("README.md", "# Checkout\n\nShipping costs 4.95 for every order.\n", in: origin)
        try write("Docs/notes.md", "# Notes\n\n- One.\n", in: origin)
        try write("Config/features.json", "{\"free_shipping\": false}\n", in: origin)
        try write("Sources/Checkout.swift", "struct Checkout {\n    var shipping: Decimal { 4.95 }\n}\n", in: origin)
        try await git(["add", "."])
        try await git(["commit", "-qm", "Baseline"])
        try await git(["worktree", "add", "-qb", "fold", worktree])
        try write("README.md", (0..<40).map { "Line \($0) of the readme, rewritten.\n" }.joined(), in: worktree)
        try write("Docs/notes.md", "# Notes\n\n- One.\n- Two.\n- Three.\n", in: worktree)
        try write("Config/features.json", "{\"free_shipping\": true, \"threshold\": 50}\n", in: worktree)
        try write("Sources/Checkout.swift", (0..<40).map { "let checkoutLine\($0) = \($0)\n" }.joined(), in: worktree)

        let repo = try await store.upsert(Repo(name: "Fold probe", path: origin))
        let workspace = try await store.upsert(Workspace(
            repoID: repo.id, name: "Fold", branch: "fold", path: worktree, baseBranch: "main"
        ))
        await app.reload()
        return WorkspaceModel(workspace: workspace, app: app)
    }

    private struct Fixture: View {
        let model: WorkspaceModel

        var body: some View {
            if let tab = CenterTabStore.shared.review(for: model.workspace.id) {
                AllFilesReviewView(model: model, selectedPath: tab.path,
                                   navigationRevision: tab.reviewNavigationRevision)
            }
        }
    }
}
#endif
