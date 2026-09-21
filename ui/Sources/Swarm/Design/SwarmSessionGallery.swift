import SwiftUI
import SwarmCore

/// The swarm session's right hand column and its document pane, on one page.
///
/// It exists because none of this is testable and the first version shipped with a layout fault a
/// picture would have caught in a second: the whole session floated in the middle of the window
/// with 540 points of empty above it, because the document pane's empty state is not greedy and
/// nothing above it filled the pane. That is not a claim a unit test can make. `CrewMessageGallery`
/// argues the same case at more length.
///
/// **Two things here are deliberately not photographs.** `DocumentPreviewView` is a `WKWebView`
/// behind an `NSViewRepresentable`, which `ImageRenderer` cannot draw, so the document pane is
/// shown in its missing-file state, which is the state that has layout worth checking. And the
/// composer inside an open agent row draws its reason for being disabled, because the fixture bus
/// answers nothing: that is the truth about this page rather than a fault in it.
struct SwarmSessionGallery: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            // Closed, which is how it is drawn every time the pane opens, and counting only the
            // two agents of the three that have reported.
            group("The agents, under the chair's conversation and in its scroll. Read only.") {
                SwarmSessionAgentReport(reader: Self.reader)
                    .frame(width: 560)
                    .background(Palette.surface)
                    .border(Palette.border)
            }

            // The rows themselves, which nothing else on this page photographs now that the block
            // above starts closed.
            group("One agent's summary, as the block draws it when it is opened.") {
                VStack(alignment: .leading, spacing: Metrics.spacingWide) {
                    ForEach(Self.digests.filter { $0.latestSummary != nil }) { digest in
                        SwarmSessionAgentSummaryRow(digest: digest)
                    }
                }
                .padding(TranscriptLayout.inset)
                .frame(width: 560)
                .background(Palette.surface)
                .border(Palette.border)
            }

            group("The changed files in the session's own folder, in the column the agents left.") {
                SwarmSessionChangeList(cwd: "/Users/x/work/swarm", files: Self.changed)
                    .frame(width: 320)
                    .background(Palette.windowBackground)
                    .border(Palette.border)
            }

            group("A document whose file has gone, which is most of the paths an old session names.") {
                SwarmSessionDocument(path: "/tmp/councils/eb29ff62/brief.md")
                    .frame(width: 520, height: 220)
                    .border(Palette.border)
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Palette.windowBackground)
    }

    @ViewBuilder
    private func group(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(Typo.caption)
                .foregroundStyle(Palette.textTertiary)
            content()
        }
    }

    // MARK: - The fixture
    //
    // Built through the real types, so a change to `SwarmSessionAgentDigest` or to the reader's
    // initialiser breaks this page rather than leaving it drawing a shape nothing produces.

    @MainActor
    private static let reader = SwarmSessionReaderModel(showing: digests, in: [session])

    private static let session = SwarmSession(
        id: SwarmSessionID("29"), talkMode: "lane", adapter: "herdr",
        cwd: "/Users/x/work/swarm", createdAt: 1_789_600_000,
        chairLog: nil, agents: 3, messages: 44, lastMessageAt: 1_789_600_900
    )

    /// Three agents, one of each liveness, because the mark is the thing this page is checked for.
    private static let digests: [SwarmSessionAgentDigest] = [
        SwarmSessionAgentDigest(
            sessionID: SwarmSessionID("29"),
            agent: SwarmAgent(
                id: SwarmAgentID("code-bus-ui2"), role: "code.complex", pane: "%3", alive: true
            ),
            latestSummary: "Read the roster from `~/.swarm/swarm.db` instead of the CLI. "
                + "Eight process launches a second became none.",
            conversation: [
                message(seq: 12, sender: "orchestrator", recipient: "code-bus-ui2",
                        body: "round: 2 Read the brief at /Users/x/.swarm/ws/ui2/fix-r2.md "
                            + "and follow it exactly."),
                message(seq: 13, sender: "code-bus-ui2", recipient: "orchestrator",
                        body: "round: 2 result: /Users/x/.swarm/ws/ui2/swarm-bus-r2.md"),
            ]
        ),
        SwarmSessionAgentDigest(
            sessionID: SwarmSessionID("29"),
            agent: SwarmAgent(
                id: SwarmAgentID("review-ui2"), role: "review", pane: "%4", alive: false
            ),
            latestSummary: "Two of nine sessions had a null `chair_log` that the CLI found anyway, "
                + "so `sessions()` stays on the CLI.",
            conversation: []
        ),
        SwarmSessionAgentDigest(
            sessionID: SwarmSessionID("29"),
            agent: SwarmAgent(
                id: SwarmAgentID("council-gemini-29"), role: "council.gemini", pane: nil, alive: nil
            ),
            latestSummary: nil,
            conversation: []
        ),
    ]

    /// One file of each layer and each status letter, because the letter and its tint are what
    /// this column is checked for.
    private static let changed: [ChangedFile] = [
        ChangedFile(path: "ui/Sources/Swarm/Views/Center/Panes/SwarmSessionView.swift",
                    change: .modified, additions: 118, deletions: 83, layer: .staged),
        ChangedFile(path: "ui/Sources/Swarm/State/SwarmSessionChangesModel.swift",
                    change: .added, additions: 84, deletions: 0, layer: .staged),
        ChangedFile(path: "ui/Sources/Swarm/Views/Transcript/TranscriptListView.swift",
                    change: .modified, additions: 9, deletions: 1, layer: .unstaged),
        ChangedFile(path: "ui/Sources/Swarm/Design/SwarmSessionAgentsView.swift",
                    change: .deleted, additions: 0, deletions: 66, layer: .unstaged),
        ChangedFile(path: "docs/bus-contract.md", change: .untracked, additions: 41,
                    layer: .untracked),
        ChangedFile(path: "ui/Resources/mark.png", change: .untracked, isBinary: true,
                    layer: .untracked),
    ]

    private static func message(
        seq: Int, sender: String, recipient: String, body: String
    ) -> SwarmMessage {
        SwarmMessage(
            seq: seq, sender: SwarmAgentID(sender), recipient: SwarmAgentID(recipient),
            kind: sender == "orchestrator" ? "ask" : "summary", body: body,
            createdAt: 1_789_600_500, read: true
        )
    }
}
