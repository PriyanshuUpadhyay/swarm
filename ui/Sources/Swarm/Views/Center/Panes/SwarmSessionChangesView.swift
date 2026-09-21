import SwiftUI
import SwarmCore

/// What is uncommitted in a swarm session's own folder, in the half of the split the agents used
/// to hold.
///
/// **Why this is not `UncommittedFileList`.** That view, the diff under it, the revert and the
/// review pane all take a `WorkspaceModel`, and a session started on the command line has no
/// workspace row to build one from. `SwarmSessionChangesModel` says at more length why inventing
/// one would be inventing a base branch too. So this is a list and a count: the file, what
/// happened to it and how much of it moved, with the editor a click away for the rest.
///
/// **Its three empty answers are three different facts**, and the middle one is the common case on
/// this Mac: most sessions run from the home directory, which is not a checkout at all. Saying
/// "no changes" there would be a lie about a folder git was never asked about.
struct SwarmSessionChangesView: View {
    var model: SwarmSessionChangesModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            content
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.windowBackground)
        .task { await model.follow() }
    }

    /// The same height as the centre column's own first band, so the two panes start their first
    /// line together and the rule between them runs straight across the join. `InspectorView` draws
    /// its top band the same way and for the same reason.
    private var header: some View {
        HStack(spacing: Metrics.spacingSmall) {
            Text("Changes")
                .font(Typo.labelEmphasis)
                .foregroundStyle(Palette.textPrimary)
            Spacer(minLength: Metrics.spacingSmall)
            if !model.files.isEmpty {
                Text(model.files.count.formatted())
                    .font(Typo.caption)
                    .monospacedDigit()
                    .foregroundStyle(Palette.textTertiary)
            }
        }
        .padding(.horizontal, Metrics.inset)
        .frame(height: Metrics.barHeight)
        .help(model.cwd)
        .overlay(alignment: .bottom) { Hairline() }
    }

    @ViewBuilder
    private var content: some View {
        if !model.isRepository {
            EmptyStateView(
                glyph: "folder",
                title: "Not a repository",
                message: "This session runs in \(abbreviated), which git does not track."
            )
        } else if let failure = model.failure {
            EmptyStateView(
                glyph: "exclamationmark.triangle",
                title: "Could not read changes",
                message: failure,
                actionTitle: "Try again",
                action: { Task { await model.refresh() } }
            )
        } else if !model.hasRead {
            LoadingView("Reading uncommitted changes")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if model.files.isEmpty {
            EmptyStateView(
                glyph: "checkmark.circle",
                title: "No uncommitted changes",
                message: "Everything in \(abbreviated) is committed."
            )
        } else {
            ScrollView {
                SwarmSessionChangeList(cwd: model.cwd, files: model.files)
            }
        }
    }

    private var abbreviated: String {
        (model.cwd as NSString).abbreviatingWithTildeInPath
    }
}

/// The rows themselves, apart from the scroller that holds them.
///
/// **Split out so a picture can be taken of it.** `ImageRenderer` proposes no height to a
/// `ScrollView`, so a column photographed whole comes out as an empty box with a header on it.
/// `SwarmSessionGallery` draws this directly and the app wraps it in the scroller, so what is
/// photographed is the list the app runs rather than a copy of it.
struct SwarmSessionChangeList: View {
    var cwd: String
    var files: [ChangedFile]

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
            ForEach(ChangeLayer.allCases, id: \.self) { layer in
                let group = files.filter { $0.layer == layer }
                if !group.isEmpty {
                    Section {
                        ForEach(group) { file in
                            SwarmSessionChangeRow(cwd: cwd, file: file)
                        }
                    } header: {
                        band(layer.title, count: group.count)
                    }
                }
            }
        }
        .padding(.bottom, Metrics.spacingSmall)
    }

    private func band(_ title: String, count: Int) -> some View {
        HStack(spacing: Metrics.spacingSmall) {
            Text(title)
            Spacer(minLength: Metrics.spacingSmall)
            Text(count.formatted()).monospacedDigit()
        }
        .font(Typo.caption)
        .foregroundStyle(Palette.textSecondary)
        .padding(.horizontal, Metrics.inset)
        .padding(.vertical, Metrics.spacingSmall)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surfaceSunken)
    }
}

/// One changed file: git's own status letter, the name, its folder, and how much of it moved.
struct SwarmSessionChangeRow: View {
    var cwd: String
    var file: ChangedFile

    @State private var isHovered = false

    var body: some View {
        Button(action: open) {
            HStack(spacing: Metrics.spacing) {
                glyph
                VStack(alignment: .leading, spacing: 0) {
                    Text(file.filename)
                        .font(Typo.body)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if !file.directory.isEmpty {
                        Text(file.directory)
                            .font(Typo.micro)
                            .foregroundStyle(Palette.textTertiary)
                            .lineLimit(1)
                            .truncationMode(.head)
                    }
                }
                Spacer(minLength: Metrics.spacingSmall)
                if file.isBinary {
                    Text("binary")
                        .font(Typo.micro)
                        .foregroundStyle(Palette.textTertiary)
                } else {
                    DiffStatLabel(additions: file.additions, deletions: file.deletions)
                }
            }
            .padding(.horizontal, Metrics.inset)
            .padding(.vertical, Metrics.spacingSmall)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .rowBackground(isSelected: false, isHovered: isHovered)
        .onHover { isHovered = $0 }
        .help(file.path)
        .accessibilityInputLabels([file.filename])
        .contextMenu {
            Button("Copy Path") { Clipboard.copy(file.path) }
        }
    }

    /// The status letter git uses, so the list reads the same as `git status` does. The letter is
    /// carried by shape as well as colour, which keeps it readable with Differentiate Without
    /// Colour turned on. `ChangedFileRow` draws the same letter for the same reason.
    private var glyph: some View {
        Text(file.change.rawValue)
            .font(Typo.codeTiny)
            .foregroundStyle(tint)
            .frame(width: Metrics.glyph, height: Metrics.glyph)
            .background(
                tint.opacity(InspectorLayout.tintOpacity),
                in: RoundedRectangle(cornerRadius: Metrics.cornerSmall)
            )
            .accessibilityLabel(description)
    }

    private var tint: Color {
        switch file.change {
        case .added, .untracked: Palette.positive
        case .deleted: Palette.negative
        case .modified: Palette.warning
        case .renamed, .copied: Palette.accent
        }
    }

    private var description: String {
        switch file.change {
        case .added: "Added"
        case .untracked: "Untracked"
        case .deleted: "Deleted"
        case .modified: "Modified"
        case .renamed: "Renamed"
        case .copied: "Copied"
        }
    }

    /// The editor, because there is no diff pane here to open into. A deleted file has nothing left
    /// on disk to open, so it opens nothing rather than raising an editor on a missing path.
    private func open() {
        guard file.change != .deleted else { return }
        Reveal.inEditor((cwd as NSString).appendingPathComponent(file.path))
    }
}
