import AppKit
import WebKit
import SwarmCore

/// One previewed file's web view, and what keeps it current while an agent rewrites the file.
///
/// A class the view holds rather than a view that owns a `WKWebView`, for the reason
/// `BrowserSession` gives: SwiftUI rebuilds representables freely, and a rebuilt web view is a page
/// loaded again from the top.
///
/// # Keeping the reader's place
///
/// A reload is the whole page going and coming back, so the scroll position is carried across it.
/// It is reported continuously by a listener in Swarm's own script world rather than asked for
/// before each reload, because the ask is asynchronous and the answer could arrive after the new
/// document has already committed at the top. The same record is what a preview reopened on the
/// same file starts from, so flipping to Source and back returns to the same paragraph.
///
/// Restoring is tried more than once. A Mermaid diagram or a report's own script lays itself out
/// after the load finishes, so the page may not yet be tall enough to scroll to where the reader
/// was; the script stops trying as soon as it gets there or the reader scrolls.
@MainActor
final class DocumentPreviewSession {
    let webView: WKWebView
    let host = BrowserHostView()
    private let schemes: DocumentPreviewSchemeHandler
    private let navigation = DocumentPreviewNavigator()
    private let scroll = DocumentPreviewScrollListener()
    private let document: String
    private let root: String
    private var fingerprint: String?

    /// Scroll positions by file, for the launch. A preview is torn down every time the tab shows
    /// source or another file, and the reader's place should not go with it.
    private static var positions: [String: CGPoint] = [:]

    /// Where a click on another worktree file goes.
    var openFile: ((String) -> Void)?

    init(document: String, root: String) {
        self.document = document
        self.root = root
        schemes = DocumentPreviewSchemeHandler(root: root, document: document)

        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(schemes, forURLScheme: DocumentPreview.scheme)
        configuration.websiteDataStore = .nonPersistent()
        let controller = configuration.userContentController
        controller.add(scroll, contentWorld: .defaultClient, name: DocumentPreviewScrollListener.name)
        controller.addUserScript(WKUserScript(
            source: DocumentPreviewScrollListener.source, injectionTime: .atDocumentEnd,
            forMainFrameOnly: true, in: .defaultClient
        ))
        webView = WKWebView(frame: .zero, configuration: configuration)
        webView.underPageBackgroundColor = NSColor(Palette.surface)
        // Transparent, so a Markdown page sits on the pane's own surface. An HTML report that
        // paints its own background still does.
        webView.setValue(false, forKey: "drawsBackground")
        host.attach(webView)

        scroll.onScroll = { [document] point in Self.positions[document] = point }
        navigation.owner = self
        webView.navigationDelegate = navigation
    }

    /// Loads the file the first time and again whenever it, or its unsaved text, has changed.
    func update(draft: String?) {
        schemes.draft = draft
        let next = DocumentPreview.fingerprint(forFile: document, draft: draft)
        guard next != fingerprint else { return }
        fingerprint = next
        if webView.url == nil {
            guard let address = DocumentPreview.address(forFile: document, root: root) else { return }
            webView.load(URLRequest(url: address))
        } else {
            webView.reload()
        }
    }

    /// Stops the page, and its scripts with it, when the pane goes. The user content controller
    /// holds its handler strongly.
    func close() {
        webView.stopLoading()
        webView.configuration.userContentController.removeAllScriptMessageHandlers()
        webView.navigationDelegate = nil
    }

    fileprivate func decide(_ action: WKNavigationAction) -> WKNavigationActionPolicy {
        guard let target = action.request.url else { return .cancel }
        let decision = DocumentPreviewNavigation.decide(
            target: target, document: document, root: root,
            isMainFrame: action.targetFrame?.isMainFrame ?? true,
            isLinkActivated: action.navigationType == .linkActivated
        )
        switch decision {
        case .allow:
            return .allow
        case let .openFile(path):
            openFile?(path)
            return .cancel
        case let .openExternally(url):
            NSWorkspace.shared.open(url)
            return .cancel
        case .refuse:
            return .cancel
        }
    }

    fileprivate func restoreScroll() {
        guard let point = Self.positions[document], point != .zero else { return }
        webView.callAsyncJavaScript(
            DocumentPreviewScrollListener.restore, arguments: ["x": point.x, "y": point.y],
            in: nil, in: .defaultClient, completionHandler: nil
        )
    }
}

private final class DocumentPreviewNavigator: NSObject, WKNavigationDelegate {
    weak var owner: DocumentPreviewSession?

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
        owner?.decide(navigationAction) ?? .cancel
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        owner?.restoreScroll()
    }
}

/// Where the page's scroll position arrives from. In `.defaultClient`, so a report's own scripts
/// can neither see the handler nor post to it.
@MainActor
private final class DocumentPreviewScrollListener: NSObject, WKScriptMessageHandler {
    static let name = "swarmPreviewScroll"

    var onScroll: ((CGPoint) -> Void)?

    static let source = """
    (() => {
      let pending = false;
      addEventListener("scroll", () => {
        if (pending) return;
        pending = true;
        setTimeout(() => {
          pending = false;
          webkit.messageHandlers.\(name).postMessage([scrollX, scrollY]);
        }, 150);
      }, { passive: true });
    })();
    """

    /// Tries for about two seconds, and gives up early once the page is there or the reader has
    /// started scrolling themselves.
    static let restore = """
    let moved = false;
    const stop = () => { moved = true; };
    addEventListener("wheel", stop, { once: true, passive: true });
    addEventListener("keydown", stop, { once: true });
    for (const delay of [0, 50, 150, 300, 600, 1000, 2000]) {
      await new Promise(resolve => setTimeout(resolve, delay));
      if (moved) return;
      scrollTo(x, y);
      if (Math.abs(scrollY - y) < 2 && Math.abs(scrollX - x) < 2) return;
    }
    """

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let pair = message.body as? [Double], pair.count == 2 else { return }
        onScroll?(CGPoint(x: pair[0], y: pair[1]))
    }
}
