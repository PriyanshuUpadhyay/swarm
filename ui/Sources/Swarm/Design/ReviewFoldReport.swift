import Foundation

/// What the all-files review currently has folded, for the probe that checks the tick folds a file.
///
/// **Why the probe cannot simply measure the pane.** `ReviewFoldProbe` began by reading the height
/// of the review's document after each tick, which is the fact a reader sees. It is not a fact a
/// headless runner reports the same way: the review is a lazy stack, so its document height is an
/// estimate over the sections nothing has laid out yet, and on the CI machine a resize of the
/// invisible window re-estimated it from 2,760 points to 300 while nothing about the ticks had
/// changed. Green on the author's Mac, red on the one that gates the merge, which is the worse of
/// the two ways round.
///
/// So the two checks that were about the FOLD rather than about the drawing ask the view what it
/// has folded, and the heights are still measured either side of the first tick, where the number
/// is the whole point and where it has always been stable.
///
/// It is the shape `SwitchProbe.attachSidebarSelection` already uses: a debug-only hook that the
/// view writes to and only a probe reads. There is no such type in a release build, and nothing in
/// the app reads this.
#if DEBUG
@MainActor
enum ReviewFoldReport {
    /// The paths the review is drawing folded, as it last drew them.
    private(set) static var collapsed: Set<String> = []

    static func report(collapsed paths: Set<String>) {
        collapsed = paths
    }
}
#endif
