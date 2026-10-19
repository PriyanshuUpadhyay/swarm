// Agent text -> HTML. Raw HTML is off, images are off, and links open only http(s).

import MarkdownIt from "markdown-it";
import { isPatch, patchKinds } from "./transcript";

const esc = new MarkdownIt().utils.escapeHtml;

// Code blocks and tables get a header with a Copy button. The list's one click listener reads
// `data-copy`: code copies the <pre> text, a table copies its markdown source from `data-raw`.
const head = (label: string, what: string) =>
  `<div class="md-head"><span>${esc(label)}</span><button type="button" data-copy="${what}" aria-label="Copy ${what}">Copy</button></div>`;

/** A diff as one `<pre>` whose lines carry their colour class; the text stays the same. A fence
 *  with no hunk header is coloured by each line's first character. */
function patchHtml(patch: string): string {
  const lines = patch.replace(/\n$/, "").split("\n");
  const kinds = isPatch(patch)
    ? patchKinds(lines)
    : lines.map((l) => (l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : l.startsWith("@@") ? "hunk" : "ctx"));
  return `<pre><code>${lines.map((l, i) => `<span class="dl ${kinds[i]}">${esc(l)}\n</span>`).join("")}</code></pre>`;
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
    const body = lang === "diff" || lang === "patch" ? patchHtml(tokens[i].content) : defaultFence(tokens, i, opts, env, self);
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
