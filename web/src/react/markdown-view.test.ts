// Step 5(g) slice S4 (contract PAINTER-V3 R3, R4 T1, V2 items 1 and 5): MarkdownView paints the HTML that the old renderMarkdown gave
// for the same text. Each case holds the Display that the Rust map gives (rust-display/display.json) and the old HTML (original/html-1.json,
// markdown.ts at 67a78495); the comment names the input id. Both sides are normalized the same way and nothing else:
// <br/> to <br>, <hr/> to <hr>, &#x27; to ', and every LF between > and < is dropped.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Display } from "../core/api";
import painter from "../../../tests/fixtures/s6c/painter.json";
import { MarkdownView } from "./markdown-view";

type Case = { name: string; display: Display; html: string };

const cases: Case[] = [
  // s2c-original-H1: a fence with a label
  {
    name: "fence_with_label",
    display: {"blocks": [{"type": "code", "value": {"label": "rust", "text": "x\n"}}]},
    html: "<div class=\"md-code\"><div class=\"md-head\"><span>rust</span><button type=\"button\" data-copy=\"code\" aria-label=\"Copy code\">Copy</button></div><pre><code class=\"language-rust\">x\n</code></pre>\n</div>",
  },
  // s2-original-C1: a fence with no word: head "code", no language class
  {
    name: "fence_without_word",
    display: {"blocks": [{"type": "code", "value": {"label": "code", "text": "x\n"}}]},
    html: "<div class=\"md-code\"><div class=\"md-head\"><span>code</span><button type=\"button\" data-copy=\"code\" aria-label=\"Copy code\">Copy</button></div><pre><code>x\n</code></pre>\n</div>",
  },
  // s2c-original-H5: indented code: bare pre and code
  {
    name: "indented_code",
    display: {"blocks": [{"type": "code", "value": {"label": null, "text": "x\n"}}]},
    html: "<pre><code>x\n</code></pre>\n",
  },
  // s4x-code-info-words: only the first info word is the label
  {
    name: "fence_with_info_words",
    display: {"blocks": [{"type": "code", "value": {"label": "py", "text": "print(1)\n"}}]},
    html: "<div class=\"md-code\"><div class=\"md-head\"><span>py</span><button type=\"button\" data-copy=\"code\" aria-label=\"Copy code\">Copy</button></div><pre><code class=\"language-py\">print(1)\n</code></pre>\n</div>",
  },
  // s4x-code-escape: code text is escaped
  {
    name: "fence_with_markup_chars",
    display: {"blocks": [{"type": "code", "value": {"label": "js", "text": "</script><b>&amp;\n"}}]},
    html: "<div class=\"md-code\"><div class=\"md-head\"><span>js</span><button type=\"button\" data-copy=\"code\" aria-label=\"Copy code\">Copy</button></div><pre><code class=\"language-js\">&lt;/script&gt;&lt;b&gt;&amp;amp;\n</code></pre>\n</div>",
  },
  // s4x-table-align: table: data-raw, head, text-align only on aligned cells
  {
    name: "table_with_align_and_one_unaligned_column",
    display: {"blocks": [{"type": "table", "value": {"source": "| l | c | r | n |\n|:--|:-:|--:|---|\n| 1 | 2 | 3 | 4 |", "align": ["left", "center", "right", "none"], "head": [[{"type": "text", "value": "l"}], [{"type": "text", "value": "c"}], [{"type": "text", "value": "r"}], [{"type": "text", "value": "n"}]], "rows": [[[{"type": "text", "value": "1"}], [{"type": "text", "value": "2"}], [{"type": "text", "value": "3"}], [{"type": "text", "value": "4"}]]]}}]},
    html: "<div class=\"md-table\" data-raw=\"| l | c | r | n |\n|:--|:-:|--:|---|\n| 1 | 2 | 3 | 4 |\"><div class=\"md-head\"><span>Table</span><button type=\"button\" data-copy=\"table\" aria-label=\"Copy table\">Copy</button></div><div class=\"md-scroll\"><table><thead>\n<tr>\n<th style=\"text-align:left\">l</th>\n<th style=\"text-align:center\">c</th>\n<th style=\"text-align:right\">r</th>\n<th>n</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td style=\"text-align:left\">1</td>\n<td style=\"text-align:center\">2</td>\n<td style=\"text-align:right\">3</td>\n<td>4</td>\n</tr>\n</tbody>\n</table></div></div>",
  },
  // s4x-table-inline: table cells hold emphasis, code and a link
  {
    name: "table_with_inline_cells",
    display: {"blocks": [{"type": "table", "value": {"source": "| a | b |\n|---|---|\n| *x* `y` | [l](https://e.test) |", "align": ["none", "none"], "head": [[{"type": "text", "value": "a"}], [{"type": "text", "value": "b"}]], "rows": [[[{"type": "emphasis", "value": [{"type": "text", "value": "x"}]}, {"type": "text", "value": " "}, {"type": "code", "value": "y"}], [{"type": "link", "value": {"url": "https://e.test", "title": null, "children": [{"type": "text", "value": "l"}]}}]]]}}]},
    html: "<div class=\"md-table\" data-raw=\"| a | b |\n|---|---|\n| *x* `y` | [l](https://e.test) |\"><div class=\"md-head\"><span>Table</span><button type=\"button\" data-copy=\"table\" aria-label=\"Copy table\">Copy</button></div><div class=\"md-scroll\"><table><thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td><em>x</em> <code>y</code></td>\n<td><a href=\"https://e.test\" target=\"_blank\" rel=\"noopener noreferrer\">l</a></td>\n</tr>\n</tbody>\n</table></div></div>",
  },
  // s4x-link-title: a link: href, title, target and rel
  {
    name: "link_with_title",
    display: {"blocks": [{"type": "paragraph", "value": [{"type": "link", "value": {"url": "https://e.test/x", "title": "t", "children": [{"type": "text", "value": "a"}]}}]}]},
    html: "<p><a href=\"https://e.test/x\" title=\"t\" target=\"_blank\" rel=\"noopener noreferrer\">a</a></p>\n",
  },
  // s4x-link-js: a non-http link paints as text
  {
    name: "link_not_http",
    display: {"blocks": [{"type": "paragraph", "value": [{"type": "text", "value": "[a](javascript:alert(1))"}]}]},
    html: "<p>[a](javascript:alert(1))</p>\n",
  },
  // s4x-link-relative: relative and mailto links paint as text
  {
    name: "links_relative_and_mailto",
    display: {"blocks": [{"type": "paragraph", "value": [{"type": "text", "value": "[a](/relative) [b](#frag) [c]("}, {"type": "link", "value": {"url": "mailto:x@e.test", "title": null, "children": [{"type": "text", "value": "mailto:x@e.test"}]}}, {"type": "text", "value": ")"}]}]},
    html: "<p>[a](/relative) [b](#frag) [c](mailto:x@e.test)</p>\n",
  },
  // s2-original-L4: ol with start 3
  {
    name: "ordered_list_from_3",
    display: {"blocks": [{"type": "list", "value": {"ordered": true, "start": 3, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "a"}]}], [{"type": "paragraph", "value": [{"type": "text", "value": "b"}]}]]}}]},
    html: "<ol start=\"3\">\n<li>a</li>\n<li>b</li>\n</ol>\n",
  },
  // s4x-list-star-tight: a tight list has no p
  {
    name: "tight_list",
    display: {"blocks": [{"type": "list", "value": {"ordered": false, "start": null, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "a"}]}], [{"type": "paragraph", "value": [{"type": "text", "value": "b"}]}]]}}]},
    html: "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n",
  },
  // s4x-list-loose: a loose list has p in each item
  {
    name: "loose_list",
    display: {"blocks": [{"type": "list", "value": {"ordered": false, "start": null, "tight": false, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "a"}]}], [{"type": "paragraph", "value": [{"type": "text", "value": "b"}]}, {"type": "list", "value": {"ordered": false, "start": null, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "c"}]}]]}}], [{"type": "paragraph", "value": [{"type": "text", "value": "d"}]}]]}}]},
    html: "<ul>\n<li>\n<p>a</p>\n</li>\n<li>\n<p>b</p>\n<ul>\n<li>c</li>\n</ul>\n</li>\n<li>\n<p>d</p>\n</li>\n</ul>\n",
  },
  // s4x-list-ordered-nested: a tight item text, LF, then the nested list
  {
    name: "nested_ordered_list",
    display: {"blocks": [{"type": "list", "value": {"ordered": true, "start": 1, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "a"}]}, {"type": "list", "value": {"ordered": true, "start": 1, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "b"}]}]]}}], [{"type": "paragraph", "value": [{"type": "text", "value": "c"}]}]]}}]},
    html: "<ol>\n<li>a\n<ol>\n<li>b</li>\n</ol>\n</li>\n<li>c</li>\n</ol>\n",
  },
  // s2b-original-W1 (breaks): breaks mode: a soft break is br then LF
  {
    name: "soft_break_with_breaks",
    display: {"blocks": [{"type": "paragraph", "value": [{"type": "text", "value": "a"}, {"type": "break"}, {"type": "text", "value": "b"}]}]},
    html: "<p>a<br>\nb</p>\n",
  },
  // s4x-emphasis-all: em, strong, s and code
  {
    name: "emphasis_strong_strike_code",
    display: {"blocks": [{"type": "paragraph", "value": [{"type": "emphasis", "value": [{"type": "text", "value": "a"}]}, {"type": "text", "value": " "}, {"type": "emphasis", "value": [{"type": "text", "value": "b"}]}, {"type": "text", "value": " "}, {"type": "strong", "value": [{"type": "text", "value": "c"}]}, {"type": "text", "value": " "}, {"type": "strong", "value": [{"type": "text", "value": "d"}]}, {"type": "text", "value": " "}, {"type": "emphasis", "value": [{"type": "strong", "value": [{"type": "text", "value": "e"}]}]}, {"type": "text", "value": " "}, {"type": "delete", "value": [{"type": "text", "value": "f"}]}, {"type": "text", "value": " "}, {"type": "code", "value": "g"}, {"type": "text", "value": " "}, {"type": "code", "value": "h`j"}]}]},
    html: "<p><em>a</em> <em>b</em> <strong>c</strong> <strong>d</strong> <em><strong>e</strong></em> <s>f</s> <code>g</code> <code>h`j</code></p>\n",
  },
  // s4x-h-all: h1 to h6 and a seven-hash line as a paragraph
  {
    name: "all_heading_depths",
    display: {"blocks": [{"type": "heading", "value": {"depth": 1, "children": [{"type": "text", "value": "h1"}]}}, {"type": "heading", "value": {"depth": 2, "children": [{"type": "text", "value": "h2"}]}}, {"type": "heading", "value": {"depth": 3, "children": [{"type": "text", "value": "h3"}]}}, {"type": "heading", "value": {"depth": 4, "children": [{"type": "text", "value": "h4"}]}}, {"type": "heading", "value": {"depth": 5, "children": [{"type": "text", "value": "h5"}]}}, {"type": "heading", "value": {"depth": 6, "children": [{"type": "text", "value": "h6"}]}}, {"type": "paragraph", "value": [{"type": "text", "value": "####### seven"}]}]},
    html: "<h1>h1</h1>\n<h2>h2</h2>\n<h3>h3</h3>\n<h4>h4</h4>\n<h5>h5</h5>\n<h6>h6</h6>\n<p>####### seven</p>\n",
  },
  // s4x-hr-variants: hr
  {
    name: "thematic_breaks",
    display: {"blocks": [{"type": "thematic_break"}, {"type": "thematic_break"}, {"type": "thematic_break"}, {"type": "paragraph", "value": [{"type": "text", "value": "a"}]}, {"type": "thematic_break"}, {"type": "paragraph", "value": [{"type": "text", "value": "b"}]}]},
    html: "<hr>\n<hr>\n<hr>\n<p>a</p>\n<hr>\n<p>b</p>\n",
  },
  // s4x-quote-blocks: blockquote holding a list and a fence
  {
    name: "quote_with_list_and_fence",
    display: {"blocks": [{"type": "blockquote", "value": [{"type": "list", "value": {"ordered": false, "start": null, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "list"}]}]]}}, {"type": "code", "value": {"label": "js", "text": "x\n"}}]}]},
    html: "<blockquote>\n<ul>\n<li>list</li>\n</ul>\n<div class=\"md-code\"><div class=\"md-head\"><span>js</span><button type=\"button\" data-copy=\"code\" aria-label=\"Copy code\">Copy</button></div><pre><code class=\"language-js\">x\n</code></pre>\n</div></blockquote>\n",
  },
  // s4x-entities: entities are decoded, then escaped by React
  {
    name: "entities_and_escapes",
    display: {"blocks": [{"type": "paragraph", "value": [{"type": "text", "value": "& < \u00a9 # &unknown;"}]}]},
    html: "<p>&amp; &lt; \u00a9 # &amp;unknown;</p>\n",
  },
];

const normalize = (html: string) => html.replace(/<br\/>/g, "<br>").replace(/<hr\/>/g, "<hr>").replace(/&#x27;/g, "'").replace(/(?<=>)\n(?=<)/g, "");

describe("MarkdownView", () => {
  it("paints the same HTML as the old renderer for the same text", () => {
    const actual = cases.map((c) => [c.name, normalize(renderToStaticMarkup(createElement(MarkdownView, { display: c.display })))]);
    const expected = cases.map((c) => [c.name, normalize(c.html)]);
    expect(actual).toEqual(expected);
  });
});

// S6C-FENCES-V1 T2. HAND entries differ from raw text to catch reparsing.
// SSR checks the box and prefix text; written browser coverage checks VList rows.
describe("S6c fence painter", () => {
  it("diff_fence_without_hunk paints literal prefix rows without layout controls", () => {
    const display = { blocks: [painter.display.blocks[1]] } as Display;
    const html = renderToStaticMarkup(createElement(MarkdownView, { display }));
    expect(normalize(html)).toEqual('<div class="md-code"><div class="md-head"><span>patch</span><button type="button" data-copy="code" aria-label="Copy code">Copy</button></div><pre><code><span class="dl del">--- &lt;old&gt;\n</span><span class="dl add">+++ &amp;new\n</span><span class="dl ctx"> complete\n</span></code></pre></div>');
  });
  it("diff_fence_with_hunk uses the shared patch box and keeps raw Copy text", () => {
    const display = { blocks: [painter.display.blocks[0]] } as Display;
    const html = renderToStaticMarkup(createElement(MarkdownView, { display }));
    const missing = renderToStaticMarkup(createElement(MarkdownView, {
      display: { blocks: [{ type: "code", value: { label: "diff", text: "@@ -1 +1 @@\n-a\n+b\n" } }] },
    }));
    expect({
      missing: normalize(missing),
      patch: /class="patch"/.test(html),
      raw: html.includes('data-raw="' + painter.raw + '"'),
      unified: html.includes('data-diff-layout="0"'),
      split: html.includes('data-diff-layout="1"'),
      copy: html.includes('data-copy="code"'),
      oldPainter: /d2h/.test(html),
    }).toEqual({
      missing: '<div class="md-code"><div class="md-head"><span>diff</span><button type="button" data-copy="code" aria-label="Copy code">Copy</button></div><pre><code class="language-diff">@@ -1 +1 @@\n-a\n+b\n</code></pre></div>',
      patch: true, raw: true, unified: true, split: true, copy: true, oldPainter: false,
    });
  });
});
