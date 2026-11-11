import { createElement, forwardRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
import type { VListHandle, VListProps } from "virtua";
import type { Display, PatchInfo } from "../core/api";
import { MarkdownView } from "./markdown-view";
import { PatchBody } from "./patch-body";

// Use the real virtualizer's SSR mode at the browser layout boundary.
vi.mock("virtua", async (load) => {
  const actual = await load<typeof import("virtua")>();
  return { ...actual, VList: forwardRef<VListHandle, VListProps>(function SsrList(props, ref) {
    return createElement(actual.VList, { ...props, ref, ssrCount: props.data?.length });
  }) };
});

const info = {
  lines: [
    { kind: "meta", text: "--- a/old-a.ts" },
    { kind: "meta", text: "+++ b/a.ts" },
    { kind: "hunk", text: "@@ -1,1 +1,1 @@" },
    { kind: "del", text: "old-a", old: 1 },
    { kind: "add", text: "new-a", new: 1 },
    { kind: "meta", text: "--- a/old-b.ts" },
    { kind: "meta", text: "+++ b/b.ts" },
    { kind: "hunk", text: "@@ -1,1 +1,1 @@" },
    { kind: "del", text: "old-b", old: 1 },
    { kind: "add", text: "new-b", new: 1 },
  ],
  split: [{ h: 2 }, { l: 3, r: 4 }, { h: 7 }, { l: 8, r: 9 }],
} satisfies PatchInfo;

const fence = (diff: typeof info | Extract<PatchInfo, { lines: unknown }>) => {
  const display: Display = { blocks: [{ type: "code", value: { label: "diff", text: "raw copy text", diff } }] };
  return renderToStaticMarkup(createElement(MarkdownView, { display }));
};
const labels = (html: string) => [...html.matchAll(/class="pl file"[^>]*>([^<]*)</g)].map(m => m[1]);

it("labels each target above its own file only in a multi-file Markdown fence", () => {
  expect(labels(fence(info))).toEqual(["a.ts", "b.ts"]);
  for (const split of [false, true]) {
    const html = renderToStaticMarkup(createElement(PatchBody, { patch: "", info, split, fileLabels: true }));
    expect(labels(html)).toEqual(["a.ts", "b.ts"]);
    expect(html.indexOf('class="pl file">a.ts')).toBeLessThan(html.indexOf("old-a</span>"));
    expect(html.indexOf("new-a</span>")).toBeLessThan(html.indexOf('class="pl file">b.ts'));
    expect(html.indexOf('class="pl file">b.ts')).toBeLessThan(html.indexOf("old-b</span>"));
    expect(labels(fence({ lines: info.lines.slice(0, 5), split: info.split.slice(0, 2) }))).toEqual([]);
    expect(labels(renderToStaticMarkup(createElement(PatchBody, { patch: "", info, split })))).toEqual([]);
    const escaped = { ...info, lines: info.lines.map((line, i) => i === 1 ? { ...line, text: "+++ b/<b>&a.ts" } : line) };
    expect(labels(fence(escaped))).toEqual(["&lt;b&gt;&amp;a.ts", "b.ts"]);
  }
});
