import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Glyph, type GlyphForm } from "./glyph";

const render = (form: GlyphForm, size?: 16 | 12) => renderToStaticMarkup(createElement(Glyph, { form, size }));
const litDots = (html: string) => Array.from(html.matchAll(/<circle\b[^>]*data-lit="true"[^>]*cx="(\d+)" cy="(\d+)"/g), ([, x, y]) => [Number(x), Number(y)]);

describe("the glyph", () => {
  it("tells rest, working and unknown apart by label and lit dots, and has seven dots", () => {
    const rest = render("rest");
    const working = render("working");
    const unlit = render("unlit");
    for (const html of [rest, working, unlit]) {
      expect(html).toContain('role="img"');
      expect((html.match(/<circle/g) ?? []).length).toBe(7);
    }
    expect([rest, working, unlit].map((h) => h.match(/aria-label="([^"]*)"/)![1])).toEqual(["Swarm", "Working", "State unknown"]);
    const litRing = [[256, 120], [374, 188], [374, 324]];
    expect([rest, working, unlit].map(litDots)).toEqual([litRing, litRing, []]);
    expect(render("working", 12)).toContain('width="12"');
    expect(renderToStaticMarkup(createElement(Glyph, { form: "working", label: "Child is working" }))).toContain('aria-label="Child is working"');
  });
});
