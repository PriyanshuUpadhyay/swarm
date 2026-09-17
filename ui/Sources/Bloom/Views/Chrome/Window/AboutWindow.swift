import AppKit
import SwiftUI
import BloomCore

/// The window behind "About Swarm".
///
/// It replaces the standard AppKit panel so the app can keep its own mark, colours and build
/// identity. One instance is retained because an AppKit window made in code is released when it
/// closes unless told otherwise.
@MainActor
enum AboutWindow {
    private static var window: NSWindow?

    /// Opens the window, or brings the existing one forward where the user left it.
    static func show() {
        let existing = window ?? make()
        window = existing
        existing.makeKeyAndOrderFront(nil)
    }

    private static func make() -> NSWindow {
        let host = NSHostingView(rootView: AboutView())
        let size = host.fittingSize

        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.title = "About Swarm"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.standardWindowButton(.miniaturizeButton)?.isHidden = true
        window.standardWindowButton(.zoomButton)?.isHidden = true
        window.contentView = host
        window.setContentSize(size)
        window.center()
        WindowRoles.mark(window, as: .reading)
        return window
    }
}

private struct AboutView: View {
    private static let width: CGFloat = 360
    private static let markSize: CGFloat = 96
    private static let plinthTop: CGFloat = 38
    private static let plinthBottom: CGFloat = 26

    var body: some View {
        VStack(spacing: 0) {
            plinth
            Rectangle()
                .fill(Palette.border)
                .frame(height: Metrics.hairline)
            Text(verbatim: "Based on Bloom by Spatie, MIT licence.")
                .font(Typo.caption)
                .foregroundStyle(Palette.textSecondary)
                .frame(maxWidth: .infinity)
                .padding(Metrics.pane)
                .background(Palette.surface)
        }
        .frame(width: Self.width)
    }

    private var plinth: some View {
        VStack(spacing: 0) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: Self.markSize, height: Self.markSize)
                .shadow(color: .black.opacity(0.55), radius: 16, y: 9)
                .accessibilityHidden(true)

            Text(verbatim: "Swarm")
                .font(Typo.display)
                .tracking(Typo.displayTracking)
                .foregroundStyle(Brand.foam)
                .padding(.top, Metrics.spacingWide)

            Text(versionLine)
                .font(Typo.codeSmall)
                .foregroundStyle(Brand.mistDim)
                .padding(.top, Metrics.spacing)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, Self.plinthTop)
        .padding(.bottom, Self.plinthBottom)
        .background {
            ZStack {
                Brand.depth
                BrandWater()
            }
            .clipped()
            .ignoresSafeArea(edges: .top)
        }
    }

    private var versionLine: String {
        BuildIdentity.read(from: .main).line(built: BuildTimestamp.read(from: .main))
    }
}
