import CoreGraphics

/// How big the Ask Swarm panel is, the order its conversations are listed in, and the mark each
/// one wears.
///
/// # Why Ask Swarm is a panel
///
/// It was a destination: a second row under Home at the top of the sidebar, selecting it swapped
/// the centre column for the conversation. The owner's words were that "Home / Ask Swarm" spent
/// the pane's best real estate on two rows, and that Ask belongs behind a button, as an overlay,
/// the way Amp opens its own. So the two rows went, Home and Ask became buttons in the sidebar's
/// status bar beside New, and Ask opens over the window in the same ground the search panel
/// already uses. `SearchPanelWindowOverlay` is why that ground and not a SwiftUI overlay on the
/// split view: the overlay dimmed the centre column and left the inspector lit.
///
/// There is no full pane any more. It was one button away in the panel's header, and the owner
/// found the panel enough on its own, so the button, the pane and its tab strip all went.
///
/// # Why the size is here
///
/// A panel holding a transcript and a composer is a different card from the search panel's list,
/// and it needs room for a rail of conversations beside the conversation. A proportion of the
/// window clamped at both ends, for the reason `SearchPanelLayout` gives for its own width: a flat
/// number is a small card adrift in a wide window, and a bare proportion is a transcript line a
/// thousand points long on an ultrawide.
public enum AskPanelLayout {
    /// The rail of conversations down the card's leading edge. A title of about thirty characters,
    /// which is what an Ask conversation's automatic title comes out at.
    public static let railWidth: CGFloat = 220

    /// Narrow enough to sit inside the narrowest window with room either side, wide enough for the
    /// rail and a composer that does not wrap its footer.
    public static let minimumWidth: CGFloat = 640
    /// A transcript wider than this is lines somebody has to move their eyes along.
    public static let maximumWidth: CGFloat = 980
    public static let widthProportion: CGFloat = 0.62

    public static let minimumHeight: CGFloat = 420
    public static let maximumHeight: CGFloat = 820
    public static let heightProportion: CGFloat = 0.8

    /// What is always left clear around the card, so it reads as in front of the window rather
    /// than as the window.
    public static let margin: CGFloat = 32

    public static func width(inWindow window: CGFloat) -> CGFloat {
        clamp(window * widthProportion, minimum: minimumWidth, maximum: maximumWidth, room: window)
    }

    public static func height(inWindow window: CGFloat) -> CGFloat {
        clamp(window * heightProportion, minimum: minimumHeight, maximum: maximumHeight, room: window)
    }

    /// The room wins over the floor. A window shorter than the minimum plus its margins gets a card
    /// that fits, because a card cut off at the bottom has lost its composer.
    private static func clamp(_ value: CGFloat, minimum: CGFloat, maximum: CGFloat, room: CGFloat) -> CGFloat {
        let fits = max(room - margin * 2, 0)
        return min(min(max(value, minimum), maximum), fits)
    }

    /// Newest first, as Mail and Messages list a conversation.
    ///
    /// The store hands conversations back in the order they were made. A list read top down wants
    /// the other way round: the conversation just started is the one being looked for.
    public static func railOrder<ID>(_ ids: [ID]) -> [ID] {
        ids.reversed()
    }

    /// The mark a conversation's row wears, in the shape the sidebar's workspace rows use.
    ///
    /// Waiting on a person outranks running, because a turn that has stopped to ask is the one
    /// that goes nowhere until somebody looks.
    public static func status(isRunning: Bool, isAwaitingPermission: Bool) -> WorkspaceStatus? {
        if isAwaitingPermission { return .awaitingPermission }
        if isRunning { return .running }
        return nil
    }
}
