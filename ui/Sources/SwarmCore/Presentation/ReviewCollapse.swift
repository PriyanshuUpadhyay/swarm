import Foundation

/// Which files the all-files review draws folded shut, when a tick arrives or comes off.
///
/// Marking a file viewed is how a reader says "I am done with this one", and a diff that stays
/// open afterwards makes the reader scroll past work they have already read. So the tick folds
/// the file, and taking the tick off opens it again.
///
/// **The hard half is that folding is also a thing the reader does by hand, and the two must not
/// fight.** A reader who ticks a file and then deliberately opens it again has to be left alone,
/// and the pane re-renders constantly: a diff stat poll every six seconds, a neighbouring section
/// loading, the window resizing. A rule of "collapsed if viewed" would slam that file shut on the
/// next one of those.
///
/// What this answers instead is the **transition**. The caller holds the set of ticked files as it
/// was last seen and hands it back here; only the paths that have appeared in the set since then
/// are folded, and only the ones that have left it are opened. A re-render with the same ticks is
/// the same set, so it moves nothing, and a hand fold or unfold never touches the remembered set
/// and so is never undone.
///
/// `wasViewed` is nil until the marks have been read out of the store, which is the other thing
/// that would fold a file nobody touched: the set starts empty and fills in a moment later, and
/// treating that as a tick would shut every file the reader marked in an earlier pass the instant
/// the pane opened.
public enum ReviewCollapse {
    /// - Parameters:
    ///   - collapsed: the files folded shut now, by path.
    ///   - viewed: the files ticked as read now, by path.
    ///   - wasViewed: the same set as it was last seen, or nil before it has been read at all.
    public static func collapsed(
        _ collapsed: Set<String>, viewed: Set<String>, wasViewed: Set<String>?
    ) -> Set<String> {
        guard let wasViewed else { return collapsed }
        return collapsed
            .union(viewed.subtracting(wasViewed))
            .subtracting(wasViewed.subtracting(viewed))
    }
}
