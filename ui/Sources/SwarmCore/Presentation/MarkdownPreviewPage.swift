import Foundation

/// The whole page a Markdown file is previewed as: `MarkdownHTML`'s body inside a stylesheet that
/// follows the window's light or dark appearance.
public enum MarkdownPreviewPage {
    /// Mermaid is fetched rather than bundled, pinned to one release and checked against its
    /// digest, so a changed file on the CDN is refused rather than run. It is 3.6 megabytes that
    /// only a document with a diagram in it asks for, and without a network the fence stays on
    /// screen as the text it is. To move the version, change both lines together:
    /// `openssl dgst -sha384 -binary mermaid.min.js | openssl base64 -A`.
    static let mermaidScript = "https://cdn.jsdelivr.net/npm/mermaid@11.17.2/dist/mermaid.min.js"
    static let mermaidIntegrity = "sha384-EOXBFmc3gx5mb+vn0vPvvGqACToJD24hhacX5Yx+8NUUQrHIle/Qi5Bg9o3zKwW2"

    public static func html(for document: MarkdownFileDocument, title: String) -> String {
        let rendered = MarkdownHTML.render(document.text)
        var body = rendered.html
        if document.isTruncated {
            body += "<p class=\"truncated\">Showing the first \(MarkdownFileDocument.lineLimit.formatted()) lines</p>\n"
        }
        // No script of the document's own can be in the body, since `MarkdownHTML` escapes all
        // markup, and the policy says so too: the only scripts are the two below.
        let scripts = rendered.hasMermaid ? """
        <script src="\(mermaidScript)" integrity="\(mermaidIntegrity)" crossorigin="anonymous"></script>
        <script>
        if (window.mermaid) {
          mermaid.initialize({ startOnLoad: false, securityLevel: "strict",
            theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "default" });
          mermaid.run({ querySelector: "pre.mermaid" });
        }
        </script>
        """ : ""
        return """
        <!doctype html>
        <html>
        <head>
        <meta charset="utf-8">
        <meta name="color-scheme" content="light dark">
        <meta http-equiv="Content-Security-Policy" content="default-src 'self'; img-src * data: blob:; media-src *; style-src 'self' 'unsafe-inline'; script-src 'unsafe-inline' https://cdn.jsdelivr.net; font-src * data:">
        <title>\(MarkdownHTML.escape(title))</title>
        <style>\(stylesheet)</style>
        </head>
        <body>
        <main class="markdown-body">
        \(body)</main>
        \(scripts)
        </body>
        </html>
        """
    }

    static let stylesheet = """
    :root {
      --text: #1f2328; --secondary: #59636e; --border: #d1d9e0; --faint: #f6f8fa;
      --accent: #0969da; --code: rgba(129, 139, 152, 0.14);
      --keyword: #cf222e; --string: #0a3069; --number: #0550ae; --comment: #6e7781;
      --function: #8250df; --type: #953800; --attribute: #116329;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --text: #e6edf3; --secondary: #9198a1; --border: #3d444d; --faint: rgba(110, 118, 129, 0.1);
        --accent: #4493f8; --code: rgba(101, 108, 118, 0.25);
        --keyword: #ff7b72; --string: #a5d6ff; --number: #79c0ff; --comment: #8b949e;
        --function: #d2a8ff; --type: #ffa657; --attribute: #7ee787;
      }
    }
    html { background: transparent; }
    body { margin: 0; color: var(--text); font: 14px/1.6 -apple-system, BlinkMacSystemFont, sans-serif;
      -webkit-font-smoothing: antialiased; word-wrap: break-word; }
    .markdown-body { max-width: 880px; margin: 0 auto; padding: 24px 32px 64px; }
    .markdown-body > :first-child { margin-top: 0; }
    h1, h2, h3, h4, h5, h6 { margin: 1.5em 0 0.6em; font-weight: 600; line-height: 1.25; }
    h1 { font-size: 2em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border); }
    h2 { font-size: 1.5em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border); }
    h3 { font-size: 1.25em; } h4 { font-size: 1em; } h5 { font-size: 0.875em; }
    h6 { font-size: 0.85em; color: var(--secondary); }
    p, blockquote, ul, ol, table, pre { margin: 0 0 1em; }
    a { color: var(--accent); text-decoration: none; }
    a:hover { text-decoration: underline; }
    ul, ol { padding-left: 2em; }
    li + li { margin-top: 0.25em; }
    ul.task-list { list-style: none; padding-left: 0.5em; }
    .task-list-item input { margin: 0 0.4em 0.2em 0; vertical-align: middle; }
    blockquote { padding: 0 1em; color: var(--secondary); border-left: 0.25em solid var(--border); }
    hr { height: 0.25em; margin: 1.5em 0; border: 0; background: var(--border); }
    img { max-width: 100%; }
    code { font: 0.85em ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--code);
      padding: 0.2em 0.4em; border-radius: 6px; }
    pre { background: var(--faint); padding: 14px 16px; border-radius: 8px; overflow: auto; line-height: 1.45; }
    pre code { background: none; padding: 0; font-size: 0.85em; }
    pre.mermaid { background: none; text-align: center; font: 0.85em ui-monospace, Menlo, monospace; }
    table { border-collapse: collapse; display: block; width: max-content; max-width: 100%; overflow: auto; }
    th, td { padding: 6px 13px; border: 1px solid var(--border); }
    th { font-weight: 600; }
    tr:nth-child(2n) { background: var(--faint); }
    .truncated { color: var(--secondary); font-size: 0.85em; }
    .tok-keyword { color: var(--keyword); } .tok-string, .tok-regex { color: var(--string); }
    .tok-number, .tok-constant { color: var(--number); } .tok-comment { color: var(--comment); font-style: italic; }
    .tok-function { color: var(--function); } .tok-type { color: var(--type); }
    .tok-attribute, .tok-variable { color: var(--attribute); }
    """
}
