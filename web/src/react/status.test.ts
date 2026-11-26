import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";
import { StatusGlyph, type Status } from "./status";

function render(status: Status, label?: string) {
  return renderToStaticMarkup(createElement(StatusGlyph, { status, label }));
}

it("shows waiting and failed marks for the stored blocked and error states", () => {
  expect(render("blocked")).toMatch(/class="[^"]*\bglyph waiting"[^>]*aria-label="waiting"[^>]*data-tip="Waiting"/);
  expect(render("error")).toMatch(/class="[^"]*\bglyph failed"[^>]*aria-label="failed"[^>]*data-tip="Failed"/);
});

it("uses the working glyph without losing the supplied label or tooltip", () => {
  const mark = render("working", "Child is working");
  expect(mark).toContain('data-form="working"');
  expect(mark).toContain('width="12"');
  expect(mark).toContain('aria-label="Child is working"');
  expect(mark).toContain('data-tip="Child is working"');
});
