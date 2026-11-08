// Agent text -> HTML. Raw HTML is off, images are off, and links open only http(s).

import { html as diffHtml } from "diff2html";
import MarkdownIt from "markdown-it";
import { isPatch } from "./transcript";

const esc = new MarkdownIt().utils.escapeHtml;

// Code blocks and tables get a header with a Copy button. The list's one click listener reads
// `data-copy`: code copies its `data-raw` source or else the <pre> text, and a table copies its
// markdown source from `data-raw`.
const head = (label: string, what: string) =>
  `<div class="md-head"><span>${esc(label)}</span><button type="button" data-copy="${what}" aria-label="Copy ${what}">Copy</button></div>`;

/** A diff fence with hunks, rendered by diff2html in both layouts. CSS shows the layout that the
 *  `data-diff-split` attribute on <html> picks. Agents often write hunks with wrong counts or with
 *  no `---`/`+++` header. diff2html still reads wrong counts, but it drops a hunk that has no file
 *  header, so a bare fence gets a placeholder header, which CSS hides. */
export function diffFence(patch: string): { headed: boolean; html: string } {
  const headed = /^--- /m.test(patch.slice(0, patch.search(/^@@ /m)));
  const src = headed ? patch : `--- a/patch\n+++ b/patch\n${patch}`;
  const layout = (outputFormat: "line-by-line" | "side-by-side") => diffHtml(src, { drawFileList: false, outputFormat });
  return {
    headed,
    html: `<div class="unified">${layout("line-by-line")}</div><div class="split">${layout("side-by-side")}</div>`,
  };
}

/** Inner HTML for a diff fence with no hunk; lines are coloured by their first character. */
export function prefixHtml(patch: string): string {
  const kind = (l: string) => (l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : l.startsWith("@@") ? "hunk" : "ctx");
  const lines = patch.replace(/\n$/, "").split("\n");
  return `<code>${lines.map((l) => `<span class="dl ${kind(l)}">${esc(l)}\n</span>`).join("")}</code>`;
}

function make(breaks: boolean) {
  const md = new MarkdownIt({ html: false, linkify: true, breaks });
  md.disable("image");
  md.validateLink = (url) => /^https?:\/\//i.test(url.trim());

  const defaultLinkOpen = md.renderer.rules.link_open ?? ((tokens, i, opts, _env, self) => self.renderToken(tokens, i, opts));
  md.renderer.rules.link_open = (tokens, i, opts, env, self) => {
    tokens[i].attrSet("target", "_blank");
    tokens[i].attrSet("rel", "noopener noreferrer");
    return defaultLinkOpen(tokens, i, opts, env, self);
  };

  const defaultFence = md.renderer.rules.fence!;
  md.renderer.rules.fence = (tokens, i, opts, env, self) => {
    const lang = tokens[i].info.trim().split(/\s+/)[0];
    const diff = lang === "diff" || lang === "patch";
    if (diff && isPatch(tokens[i].content)) {
      const { headed, html } = diffFence(tokens[i].content);
      const seg =
        '<div class="seg" role="group" aria-label="Diff layout"><button type="button" data-diff-layout="0">Unified</button><button type="button" data-diff-layout="1">Split</button></div>';
      // The rendered table is not the patch text, so Copy reads `data-raw`.
      return `<div class="md-code" data-raw="${esc(tokens[i].content)}">${head(lang, "code").replace("<button", `${seg}<button`)}<div class="d2h${headed ? "" : " bare"}">${html}</div></div>`;
    }
    const body = diff ? `<pre>${prefixHtml(tokens[i].content)}</pre>` : defaultFence(tokens, i, opts, env, self);
    return `<div class="md-code">${head(lang || "code", "code")}${body}</div>`;
  };

  md.renderer.rules.table_open = (tokens, i, _opts, env) => {
    const map = tokens[i].map;
    const src = (env as { lines?: string[] } | undefined)?.lines;
    const raw = map && src ? src.slice(map[0], map[1]).join("\n") : "";
    return `<div class="md-table" data-raw="${esc(raw)}">${head("Table", "table")}<div class="md-scroll"><table>`;
  };
  md.renderer.rules.table_close = () => "</table></div></div>";
  return md;
}

const agent = make(false);
// The owner's own messages keep each typed line break.
const owner = make(true);

export function renderMarkdown(text: string, breaks = false): string {
  return (breaks ? owner : agent).render(text, { lines: text.split("\n") });
}

/** HTML per row id. A row renders again only when its text changed (a merged chunk). */
export class MarkdownCache {
  private map = new Map<string, { text: string; html: string }>();

  html(id: string, text: string, breaks = false): string {
    const hit = this.map.get(id);
    if (hit && hit.text === text) return hit.html;
    const html = renderMarkdown(text, breaks);
    this.map.set(id, { text, html });
    return html;
  }
}
