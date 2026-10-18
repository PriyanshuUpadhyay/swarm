import { expect, test } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import type { Snapshot } from "../src/core/api";

test("pi keeps menu picks as chips and runs the first after a reload without a list", async ({ page: tab }, info) => {
  const session = process.env.SWARM_E2E_LIVE!;
  const errors: string[] = [];
  // WebKit reports a fetch that a reload cancels as "... due to access control checks".
  const expected = (m: string) => m.startsWith("ResizeObserver loop") || m.endsWith("due to access control checks.");
  tab.on("pageerror", (e) => !expected(e.message) && errors.push(e.message));
  // Use the live composer fixture with pi's provider and skill-list responses.
  await tab.route("**/api/snapshot", async (route) => {
    const snapshot: Snapshot = await (await route.fetch()).json();
    snapshot.sessions.find((s) => s.id === session)!.chair_provider = "pi";
    snapshot.agents[session].forEach((a) => { a.provider = "pi"; });
    await route.fulfill({ json: snapshot });
  });
  let lists = 0;
  let listFails = false;
  await tab.route("**/api/chat/commands?*", (route) => {
    lists++;
    return listFails ? route.fulfill({ status: 503, body: "probe failed" }) : route.fulfill({ json: {
      commands: ["plan", "research"].map((name) => ({ name: `skill:${name}`, insert: `/skill:${name}`, kind: "skill", description: `Run ${name}`, hint: "", source: "", dialog: false })),
      note: null,
    } });
  });
  const sent: string[] = [];
  let sendFails = true;
  await tab.route("**/api/chat/send", (route) => {
    sent.push(route.request().postDataJSON().text);
    return route.fulfill(sendFails ? { status: 409, body: "send failed" } : { json: { ok: true } });
  });
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  const composer = tab.getByTestId("composer");
  const chips = tab.getByTestId("composer-skill-chip");
  await composer.fill("fix this /ski");
  await expect(tab.getByTestId("slash-row").filter({ hasText: "/skill:plan" })).toBeVisible();
  await tab.keyboard.press("Tab");
  await tab.keyboard.type("now");
  await expect(composer).toHaveValue("fix this /skill:plan now");
  await expect(chips).toHaveText(["/skill:plan"]);

  await tab.keyboard.type(" /skill:res");
  await expect(tab.getByTestId("slash-row").filter({ hasText: "/skill:research" })).toBeVisible();
  await tab.keyboard.press("Tab");
  await expect(chips).toHaveCount(2);
  await expect(chips.nth(1)).toHaveAttribute("data-muted", "true");
  await expect(tab.getByTestId("composer-status")).toContainText("pi runs one skill per message");
  expect(await chips.nth(0).evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(await chips.nth(1).evaluate((el) => getComputedStyle(el).backgroundColor));
  await composer.evaluate((el: HTMLTextAreaElement) => el.setSelectionRange("fix this /skill:plan".length, el.value.length));
  await tab.keyboard.insertText("\nnow");
  await expect(chips).toHaveText(["/skill:plan"]);
  // A chat switch and a page reload both restore the same picked occurrence.
  await tab.locator(`[data-testid="tree-row"][data-kind="chat"]:not([data-session="${session}"])`).first().click();
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(chips).toHaveText(["/skill:plan"]);
  listFails = true;
  await tab.reload();
  await tab.waitForFunction(() => window.__swarmReady === true);
  await expect(composer).toHaveValue("fix this /skill:plan\nnow");
  await expect(chips).toHaveText(["/skill:plan"]);

  const mirror = tab.getByTestId("composer-highlights");
  const metrics = await composer.evaluate((el) => {
    const style = getComputedStyle(el);
    return [el.clientWidth, style.fontFamily, style.fontSize, style.lineHeight, style.padding, style.whiteSpace, style.overflowWrap];
  });
  expect(await mirror.evaluate((el) => {
    const style = getComputedStyle(el);
    return [el.clientWidth, style.fontFamily, style.fontSize, style.lineHeight, style.padding, style.whiteSpace, style.overflowWrap];
  })).toEqual(metrics);
  // A chip has the same advance width as the textarea's plain text font.
  expect(await chips.first().evaluate((el) => {
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d")!;
    ctx.font = getComputedStyle(el).font;
    return Math.abs(el.getBoundingClientRect().width - ctx.measureText(el.textContent!).width);
  })).toBeLessThan(1);
  const tmp = resolve(import.meta.dirname, "../../tmp");
  mkdirSync(tmp, { recursive: true });
  await tab.getByTestId("composer-box").screenshot({ path: resolve(tmp, `pi-skill-chip-${info.project.name}.png`) });

  const draft = "fix this /skill:plan\nnow";
  await composer.focus();
  await composer.evaluate((el: HTMLTextAreaElement) => el.setSelectionRange(el.value.length, el.value.length));
  await tab.keyboard.insertText("\n" + "Text that wraps in the composer. ".repeat(100));
  await composer.evaluate((el) => { el.scrollTop = el.scrollHeight; el.dispatchEvent(new Event("scroll", { bubbles: true })); });
  const scrollTop = await composer.evaluate((el) => el.scrollTop);
  expect(scrollTop).toBeGreaterThan(0);
  await expect.poll(() => mirror.evaluate((el) => el.scrollTop)).toBe(scrollTop);
  expect(await mirror.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
  await composer.evaluate((el: HTMLTextAreaElement, start) => el.setSelectionRange(start, el.value.length), draft.length);
  await tab.keyboard.press("Backspace");
  await expect(composer).toHaveValue(draft);
  await expect(chips).toHaveCount(1);

  const beforeSend = lists;
  await tab.getByTestId("send").click();
  await expect.poll(() => sent).toEqual(["/skill:plan fix this now"]);
  await expect(tab.getByTestId("chat-banner")).toBeVisible();
  await expect(chips).toHaveCount(1);
  expect(lists).toBe(beforeSend);
  sendFails = false;
  await tab.getByTestId("chat-banner").getByRole("button", { name: "Retry" }).click();
  await expect(composer).toHaveValue("");
  await expect(chips).toHaveCount(0);
  expect(sent).toEqual(["/skill:plan fix this now", "/skill:plan fix this now"]);

  listFails = false;
  await composer.fill("fix /ski");
  await expect(tab.getByTestId("slash-row").first()).toBeVisible();
  await tab.keyboard.press("Tab");
  await composer.fill("fix /skill:plans now");
  await expect(chips).toHaveCount(0);
  expect(errors).toEqual([]);
});
