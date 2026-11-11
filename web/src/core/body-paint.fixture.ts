// Test boundary: use the production row painter, with no new web API pinned.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect } from "vitest";
import type { Row } from "./api";
import type { Reader } from "./app";
import { displayOf, type Line } from "./transcript";
import { LineView } from "../react/transcript-row";
import { MarkdownView } from "../react/markdown-view";

export function paintedBody(row: Row) {
  const app = { imageUrl: () => "" } as unknown as Reader;
  const line = { id: row.id, row, fold: null, open: false } as Line;
  const html = renderToStaticMarkup(createElement(LineView, { app, line, index: 0, full: new Map() }));
  if (html.includes('<div class="md">')) {
    const display = displayOf(row)!;
    // Check that the chosen tree, rather than row.text, reached the renderer.
    const tree = renderToStaticMarkup(createElement(MarkdownView, { display }));
    expect(html).toContain('<div class="md">' + tree + '</div>');
    return { kind: "tree", display };
  }
  const escaped = row.text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#x27;");
  expect(html).toContain('<div class="whitespace-pre-wrap">' + escaped + '</div>');
  return { kind: "plain" };
}
