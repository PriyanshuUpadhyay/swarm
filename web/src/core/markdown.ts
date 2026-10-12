// Agent text -> HTML. Raw HTML is off, images are off, and links open only http(s).

import MarkdownIt from "markdown-it";

const md = new MarkdownIt({ html: false, linkify: true });
md.disable("image");
md.validateLink = (url) => /^https?:\/\//i.test(url.trim());

const defaultLinkOpen = md.renderer.rules.link_open ?? ((tokens, i, opts, _env, self) => self.renderToken(tokens, i, opts));
md.renderer.rules.link_open = (tokens, i, opts, env, self) => {
  tokens[i].attrSet("target", "_blank");
  tokens[i].attrSet("rel", "noopener noreferrer");
  return defaultLinkOpen(tokens, i, opts, env, self);
};

export function renderMarkdown(text: string): string {
  return md.render(text);
}

/** HTML per row id. A row renders again only when its text changed (a merged chunk). */
export class MarkdownCache {
  private map = new Map<string, { text: string; html: string }>();

  html(id: string, text: string): string {
    const hit = this.map.get(id);
    if (hit && hit.text === text) return hit.html;
    const html = renderMarkdown(text);
    this.map.set(id, { text, html });
    return html;
  }
}
