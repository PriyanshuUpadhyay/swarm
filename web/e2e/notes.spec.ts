import { expect, test, type Page } from "@playwright/test";
import { mockRows, open, row, rows } from "./ux-helpers";

// Chat notes (ADR 0161): the owner marks agent reply text, writes a note in a numbered card, and
// the agent gets every note of the chat as one typed message.

const REPLY = "The page cache keeps each rendered page for 10 minutes, so an edit shows late.\n\nReply with A, B, or C.";

/** The texts that the composer's send posts, answered as sent. */
function captureSends(tab: Page) {
  const sent: Record<string, unknown>[] = [];
  void tab.route("**/api/chat/send", (route) => {
    sent.push(route.request().postDataJSON());
    return route.fulfill({ json: { ok: true } });
  });
  return sent;
}

/** Selects `text` inside the first element that matches `selector` and holds it. */
async function select(tab: Page, selector: string, text: string) {
  await tab.locator(selector).filter({ hasText: text }).first().evaluate((el, text) => {
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let node = walk.nextNode(); node; node = walk.nextNode()) {
      const at = (node as Text).data.indexOf(text);
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + text.length);
      const sel = window.getSelection()!;
      sel.removeAllRanges();
      sel.addRange(range);
      return;
    }
    throw new Error(`no text ${text}`);
  }, text);
}

test("a text note from the row menu goes to the agent as one message with its quote", async ({ page: tab }) => {
  await mockRows(tab, [row("a1", REPLY)]);
  const sent = captureSends(tab);
  await open(tab);
  const reply = '[data-testid="row"][data-id="a1"] .md p';
  await select(tab, reply, "10 minutes");
  await tab.locator(reply).first().click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Add note" }).click();

  const card = tab.getByTestId("note-card");
  await expect(card).toHaveCount(1);
  await expect(card).toContainText("10 minutes");
  await expect(tab.getByTestId("note-input")).toBeFocused();
  await tab.keyboard.type("Why 10 minutes?");
  await expect(tab.getByTestId("notes-bar")).toContainText("1 note on this chat");

  await tab.getByRole("button", { name: "Send 1 note" }).click();
  await expect.poll(() => sent.length).toBe(1);
  expect(sent[0].text).toBe("Notes on your reply (1):\n\n1. > 10 minutes\n   Why 10 minutes?");
  await expect(card).toHaveCount(0);
  await expect(tab.getByTestId("notes-bar")).toBeHidden();
});

/** A reply with a heading, a list of choices and a closing line, as the server renders it. */
async function mockChoices(tab: Page) {
  const text = "Which design goes to the build?\n\n- A · Notes tray\n- B · Quote into the composer\n- C · Margin notes\n\nReply with A, B, or C.";
  const page = rows([row("a1", text)]);
  const p = (value: string) => ({ type: "paragraph" as const, value: [{ type: "text" as const, value }] });
  page.display = { a1: { blocks: [
    p("Which design goes to the build?"),
    { type: "list", value: { ordered: false, start: null, tight: true, items: [[p("A · Notes tray")], [p("B · Quote into the composer")], [p("C · Margin notes")]] } },
    p("Reply with A, B, or C."),
  ] } };
  page.body = { a1: "tree" };
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: page }));
  return text;
}

const rowTops = (tab: Page) => tab.locator('[data-testid="row"]').evaluateAll((els) => els.map((e) => e.getBoundingClientRect().top));

test("annotate mode takes an item and a whole reply; ⇧⌘A, the palette and Esc switch it; rows stay put", async ({ page: tab }) => {
  const reply = await mockChoices(tab);
  const sent = captureSends(tab);
  await open(tab);
  const mode = tab.getByTestId("annotate-mode");
  await expect(mode).toHaveAttribute("aria-pressed", "false");
  const off = await rowTops(tab);

  await tab.keyboard.press("Meta+Shift+A");
  await expect(mode).toHaveAttribute("aria-pressed", "true");
  expect(await rowTops(tab)).toEqual(off);
  await tab.keyboard.press("Escape");
  await expect(mode).toHaveAttribute("aria-pressed", "false");
  await tab.keyboard.press("Meta+k");
  await tab.keyboard.type("Annotate Chat");
  await tab.keyboard.press("Enter");
  await expect(mode).toHaveAttribute("aria-pressed", "true");
  expect(await rowTops(tab)).toEqual(off);

  await tab.locator('[data-testid="row"] li').filter({ hasText: "C · Margin notes" }).click();
  await expect(tab.getByTestId("note-input")).toBeFocused();
  await tab.keyboard.type("Go with this one.");
  await expect(tab.locator('[data-testid="row"] li[data-inote="1"]')).toHaveText("C · Margin notes");
  await tab.getByRole("button", { name: "Add a note on the whole reply" }).click();
  await expect(tab.getByTestId("note-card")).toHaveCount(2);
  await expect(mode).toHaveAttribute("aria-pressed", "true");

  await tab.getByRole("button", { name: "Send 2 notes" }).click();
  await expect.poll(() => sent.length).toBe(1);
  expect(sent[0].text).toBe([
    "Notes on your reply (2):",
    "",
    "1. > C · Margin notes",
    "   Go with this one.",
    "",
    "2. > [whole reply]",
    ...reply.split("\n").map((l) => (l ? `   > ${l}` : "   >")),
  ].join("\n"));
});
