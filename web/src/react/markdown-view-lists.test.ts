// Step 5(g) slice S4 (contract PAINTER-V4 item 2): a list item whose text is followed by a fence, indented code or a table. Same style as
// markdown-view.test.ts. Each case holds the Display from the Rust map (red-v2/display-v2.json) and the old HTML (original-v2/html-1.json).
// Both sides are normalized the same way and nothing else: <br/> to <br>, <hr/> to <hr>, &#x27; to ', and every LF between > and < is dropped.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Display } from "../core/api";
import { MarkdownView } from "./markdown-view";

type Case = { name: string; display: Display; html: string };

const cases: Case[] = [
  // v4-item-fence: a tight item: the text, then the fence with no LF between
  {
    name: "tight_item_then_fence",
    display: {"blocks": [{"type": "list", "value": {"ordered": true, "start": 1, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "Run:"}]}, {"type": "code", "value": {"label": "code", "text": "x\n"}}]]}}]},
    html: "<ol>\n<li>Run:<div class=\"md-code\"><div class=\"md-head\"><span>code</span><button type=\"button\" data-copy=\"code\" aria-label=\"Copy code\">Copy</button></div><pre><code>x\n</code></pre>\n</div></li>\n</ol>\n",
  },
  // v4-item-indented-code: indented code after a paragraph needs a blank line, so the list is loose: p, then bare pre
  {
    name: "loose_item_then_indented_code",
    display: {"blocks": [{"type": "list", "value": {"ordered": true, "start": 1, "tight": false, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "Run:"}]}, {"type": "code", "value": {"label": null, "text": "code\n"}}]]}}]},
    html: "<ol>\n<li>\n<p>Run:</p>\n<pre><code>code\n</code></pre>\n</li>\n</ol>\n",
  },
  // v4-item-table: a tight item: the text, then the table with no LF; data-raw keeps the indent
  {
    name: "tight_item_then_table",
    display: {"blocks": [{"type": "list", "value": {"ordered": true, "start": 1, "tight": true, "items": [[{"type": "paragraph", "value": [{"type": "text", "value": "Run:"}]}, {"type": "table", "value": {"source": "   | a |\n   |---|\n   | 1 |", "align": ["none"], "head": [[{"type": "text", "value": "a"}]], "rows": [[[{"type": "text", "value": "1"}]]]}}]]}}]},
    html: "<ol>\n<li>Run:<div class=\"md-table\" data-raw=\"   | a |\n   |---|\n   | 1 |\"><div class=\"md-head\"><span>Table</span><button type=\"button\" data-copy=\"table\" aria-label=\"Copy table\">Copy</button></div><div class=\"md-scroll\"><table><thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n<tbody>\n<tr>\n<td>1</td>\n</tr>\n</tbody>\n</table></div></div></li>\n</ol>\n",
  },
];

const normalize = (html: string) => html.replace(/<br\/>/g, "<br>").replace(/<hr\/>/g, "<hr>").replace(/&#x27;/g, "'").replace(/(?<=>)\n(?=<)/g, "");

describe("MarkdownView lists", () => {
  it("paints a list item followed by a fence, indented code or a table like the old renderer", () => {
    const actual = cases.map((c) => [c.name, normalize(renderToStaticMarkup(createElement(MarkdownView, { display: c.display })))]);
    const expected = cases.map((c) => [c.name, normalize(c.html)]);
    expect(actual).toEqual(expected);
  });
});
