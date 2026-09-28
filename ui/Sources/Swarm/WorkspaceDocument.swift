import SwiftUI
import WebKit

struct WorkspaceDocument: Identifiable {
    let id = UUID()
    let title: String
    let detail: String
    var isDiff = true
    let load: @Sendable () async throws -> String
}

struct WorkspaceDocumentView: View {
    let document: WorkspaceDocument
    @AppStorage("splitDiff") private var split = false
    @State private var text: String?
    @State private var error: String?
    @State private var rendered = false
    @State private var renderError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(verbatim: document.title).font(.headline).lineLimit(1).truncationMode(.middle)
                    Text(verbatim: document.detail).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                }
                Spacer()
                if document.isDiff {
                    Picker("Diff layout", selection: $split) {
                        Text("Unified").tag(false)
                        Text("Split").tag(true)
                    }.pickerStyle(.segmented).labelsHidden().frame(width: 150)
                        .onChange(of: split) { _, _ in rendered = false; renderError = nil }
                }
            }.padding(12)
            Divider()
            if let error {
                Text(verbatim: error).foregroundStyle(.red).textSelection(.enabled).padding()
                Spacer()
            } else if let text {
                ZStack {
                    DiffWebView(text: text, isDiff: document.isDiff, split: split) { success in
                        rendered = true
                        renderError = success ? nil : "The file view could not load. Close this preview and try again."
                    }
                    if !rendered {
                        ProgressView("Rendering…").frame(maxWidth: .infinity, maxHeight: .infinity)
                            .background(.background)
                    }
                    if let renderError {
                        Text(verbatim: renderError).foregroundStyle(.red).padding()
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .background(.background)
                    }
                }
            } else {
                ProgressView("Reading file…").frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(.background)
        .task(id: document.id) {
            text = nil
            error = nil
            rendered = false
            renderError = nil
            do {
                let value = try await document.load()
                try Task.checkCancellation()
                text = value
            } catch {
                if !Task.isCancelled { self.error = String(describing: error) }
            }
        }
    }
}

struct DiffWebView: NSViewRepresentable {
    let text: String
    let isDiff: Bool
    let split: Bool
    let onRendered: @MainActor (Bool) -> Void
    @Environment(\.colorScheme) private var colorScheme

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.setValue(false, forKey: "drawsBackground")
        let installed = Bundle.main.resourceURL?.appendingPathComponent("DiffViewer", isDirectory: true)
        let root = installed.flatMap { FileManager.default.fileExists(atPath: $0.appendingPathComponent("index.html").path) ? $0 : nil }
            ?? Bundle.module.url(forResource: "DiffViewer", withExtension: nil)!
        context.coordinator.root = root
        view.loadFileURL(root.appendingPathComponent("index.html"), allowingReadAccessTo: root)
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        context.coordinator.onRendered = onRendered
        let input = Input(text: text, isDiff: isDiff, split: split, dark: colorScheme == .dark)
        guard input != context.coordinator.input else { return }
        context.coordinator.input = input
        if context.coordinator.ready { context.coordinator.render(view) }
    }

    struct Input: Equatable {
        let text: String
        let isDiff: Bool
        let split: Bool
        let dark: Bool
    }

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate {
        var root: URL?
        var input: Input?
        var ready = false
        var renderID = 0
        var onRendered: (@MainActor (Bool) -> Void)?

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            ready = true
            render(webView)
        }

        func render(_ view: WKWebView) {
            guard let input else { return }
            renderID += 1
            let requestID = renderID
            // Workspace content is passed as data, never inserted into executable JavaScript.
            view.callAsyncJavaScript(
                "window.renderPreview(text, isDiff, split, dark)",
                arguments: ["text": input.text, "isDiff": input.isDiff, "split": input.split, "dark": input.dark],
                in: nil, in: .page
            ) { result in
                guard self.renderID == requestID else { return }
                switch result {
                case .success: self.onRendered?(true)
                case .failure: self.onRendered?(false)
                }
            }
        }

        func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
                     decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void) {
            let url = navigationAction.request.url
            let allowed = url?.isFileURL == true && url?.deletingLastPathComponent().standardizedFileURL == root?.standardizedFileURL
                && url?.lastPathComponent == "index.html"
            decisionHandler(allowed ? .allow : .cancel)
        }
    }
}
