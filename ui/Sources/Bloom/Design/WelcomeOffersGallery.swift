import SwiftUI
import BloomCore

/// The welcome window's command-line offer at the width the window draws it at.
///
/// It exists because of how the alternative works. The welcome window is reachable from a capture
/// run only through `--welcome`, which opens the real window on the real desktop and photographs
/// it, and that window opens on the checks: there is no flag that walks it forward, so the offer
/// has never been lookable-at without pressing a button on somebody's machine. A page
/// rendered offscreen costs nobody their focus.
///
/// The real view and its real strings are used, so the page cannot say the copy is one thing while
/// the window says another.
///
/// What it does not draw is the plinth and the footer. Those are `WelcomeView`'s and are private
/// to it, they are identical on every screen, and the checks capture already shows them.
/// The hairlines are here because they are what the band is bounded by, and a band drawn without
/// them reads as more space than it has.
///
///     Bloom --snapshot-gallery /tmp/shots --gallery welcome-offers
struct WelcomeOffersGallery: View {
    /// A command of the real shape, built the way the window builds it, so the box is the width it
    /// will actually be. Not a token from this machine: nothing in a capture run should be able to
    /// put a live one in a PNG.
    private static let command = BridgeRegistration.ownerAddCommand(BridgeAttachment(
        shimPath: "/Applications/Bloom.app/Contents/MacOS/bloom-bridge",
        socketPath: "/Users/you/Library/Application Support/Bloom/bridge.sock",
        token: "0000000000000000000000000000000000000000000000000000000000000000",
        role: .owner
    ))

    var body: some View {
        band("Only when there is something to offer") {
            WelcomeCommandLine(command: Self.command)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    /// One screen's reading band, on the ground and at the width the window gives it.
    private func band<Content: View>(
        _ title: String, @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(title)
                .font(Typo.title)
                .foregroundStyle(Palette.textPrimary)

            VStack(spacing: 0) {
                Rectangle().fill(Palette.border).frame(height: Metrics.hairline)
                content()
                Rectangle().fill(Palette.border).frame(height: Metrics.hairline)
            }
            .background(Palette.surface)
        }
        .frame(width: WelcomeOffersGallery.bandWidth, alignment: .leading)
    }

    /// `WelcomeView.width`, which is private to it and is the one number these screens are laid out
    /// against. Repeated rather than published, because widening a view's constants so a capture
    /// page can read them is the wrong trade: this is a page, and a page that is 520 when the
    /// window has moved on is a page whose caption is wrong rather than a bug in the app.
    private static let bandWidth: CGFloat = 520
}

extension Gallery {
    /// The registry entry for this page. See `Gallery`.
    static let welcomeOffers = Gallery(
        name: "welcome-offers",
        title: "The welcome window's command-line offer",
        size: CGSize(width: 584, height: 560),
        needsFocus: false,
        view: { _ in AnyView(WelcomeOffersGallery()) }
    )
}
