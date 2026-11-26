import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import { CliMark } from "./cli-mark";

test("the four CLIs use one decorative letter box", () => {
  const marks = [["claude", "C"], ["codex", "X"], ["agy", "A"], ["pi", "P"]];
  for (const [provider, letter] of marks) {
    const html = renderToStaticMarkup(createElement(CliMark, { provider }));
    expect(html).toContain(`aria-hidden="true" data-cli-mark="${provider}"`);
    expect(html).toContain("size-4");
    expect(html).toContain("text-[11px]");
    expect(html).toContain(`>${letter}</span>`);
  }
});
