import CoreGraphics

/// Where a content-sized utility window sits: centred on its owner when it opens, and pinned by
/// its top edge whenever its content changes height afterwards. Screen bounds win near an edge so
/// the title bar and primary action remain reachable.
public enum CentredWindowPlacement {
    public static func frame(size: CGSize, around anchor: CGRect, visible: CGRect) -> CGRect {
        clamped(CGRect(
            x: anchor.midX - size.width / 2, y: anchor.midY - size.height / 2,
            width: size.width, height: size.height
        ), to: visible)
    }

    /// The frame after a height change, with the top edge left where it was.
    ///
    /// The welcome window used to be resized around its centre, which kept the maths symmetrical
    /// and moved the title bar on every press: each step is a different height, so the window
    /// hopped up and down the screen "like a rabbit", in the words of the first person to walk
    /// through it on a fresh Mac. A sheet, a setup assistant and every AppKit window whose content
    /// grows keep the top still and move the bottom, so the controls somebody is looking at stay
    /// under their eyes. Only a window that would run off the bottom of the display is lifted.
    public static func frame(size: CGSize, keepingTopOf current: CGRect, visible: CGRect) -> CGRect {
        clamped(CGRect(
            x: current.midX - size.width / 2, y: current.maxY - size.height,
            width: size.width, height: size.height
        ), to: visible)
    }

    private static func clamped(_ frame: CGRect, to visible: CGRect) -> CGRect {
        let x = min(max(frame.minX, visible.minX), max(visible.minX, visible.maxX - frame.width))
        let y = min(max(frame.minY, visible.minY), max(visible.minY, visible.maxY - frame.height))
        return CGRect(origin: CGPoint(x: x, y: y), size: frame.size)
    }
}
