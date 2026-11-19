import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { loadHighlighter } from "./code-tokens";
import { MarkdownView } from "./markdown-view";

describe("code block colours", () => {
  it("colours a known language and keeps the code text whole", async () => {
    await loadHighlighter();
    const text = "const a = \"<b>\";\nfn();\n";
    const html = renderToStaticMarkup(createElement(MarkdownView, { display: { blocks: [{ type: "code", value: { label: "ts", text } }] } }));
    expect(html).toMatch(/<code class="language-ts"><span style="color:light-dark\(/);
    const code = html.match(/<code[^>]*>([\s\S]*)<\/code>/)![1].replace(/<[^>]+>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"");
    expect(code).toBe(text);
  });
});
