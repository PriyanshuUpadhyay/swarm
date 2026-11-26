import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import { LetterBox } from "./letter-box";

test("the letter box shows the supplied mark", () => {
  const html = renderToStaticMarkup(createElement(LetterBox, { letter: "R" }));
  expect(html).toMatch(/>R<\/span>/);
});
