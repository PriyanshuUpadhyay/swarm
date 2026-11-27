import { expect, test } from "@playwright/test";
import type { Row } from "../src/core/api";
import { captureThemes, open, row, rows } from "./ux-helpers";

test("a long group title is truncated inside the card", async ({ page }) => {
  const title = "Review the release and the related checks ".repeat(5).trim();
  const batch: Row = {
    ...row("sub1", title, "subagent"),
    subagent: {
      call_id: "fixture-call", other_call_ids: ["fixture-call.2"], agent_type: "subagent", description: "First child", state: "finished",
    },
  };
  await page.route("**/api/rows?*", (route) => route.fulfill({ json: rows([batch]) }));
  await open(page);
  await page.setViewportSize({ width: 850, height: 720 });

  const name = page.getByTestId("subagent-row").getByText(title, { exact: true });
  await expect(name).toBeVisible();
  await expect(name).toHaveCSS("text-overflow", "ellipsis");
  expect(await name.evaluate((el) => el.scrollWidth > el.clientWidth && el.getBoundingClientRect().right <= el.parentElement!.getBoundingClientRect().right)).toBe(true);
});

test("a group shows observed states, expands after five and opens the chosen child", async ({ page }) => {
  const batch = {
    ...row("sub1", "Review the release", "subagent"),
    subagent: {
      call_id: "fixture-call", other_call_ids: ["fixture-call.2"], agent_type: "subagent", description: "", state: "finished" as const,
      children: [
        { call_id: "fixture-call", description: "Check rendering", state: "started" },
        { call_id: "fixture-call.2", description: "Check the docs", state: "done" },
        { call_id: "fixture-call.3", description: "Check the build", state: "failed" },
        { call_id: "fixture-call.4", description: "Check the version", state: "done" },
        { call_id: "fixture-call.5", description: "Check the test report" },
        { call_id: "fixture-call.6", description: "Check keyboard navigation", state: "future-state" },
      ],
    },
  } satisfies Row & { subagent: NonNullable<Row["subagent"]> & { children: unknown[] } };

  await page.route("**/api/rows?*", (route) => {
    const agent = new URL(route.request().url()).searchParams.get("agent");
    return route.fulfill({ json: rows(agent === "chair~sub~fixture-call.6" ? [row("a1", "Keyboard fixes are ready")] : [batch]) });
  });
  await open(page);

  const card = page.getByTestId("subagent-row");
  await expect(card).toContainText("1 failed");
  await expect(card).toContainText("2 unknown");
  await expect(card.getByRole("button").first()).toContainText("Check the build");
  const child = card.getByRole("button", { name: /Check keyboard navigation/ });
  await expect(child).toHaveCount(0);
  await card.getByRole("button", { name: "Show all", exact: true }).click({ timeout: 3000 });
  await expect(child).toBeVisible();

  await captureThemes(page, card, "subagents");

  await child.click();
  await expect(page.getByTestId("subagent-chat")).toContainText("Keyboard fixes are ready");
});

test("a child message stays expanded after its log refreshes and the row remounts", async ({ page }) => {
  const batch: Row = {
    ...row("sub1", "Review the release", "subagent"),
    subagent: { call_id: "child-cut", agent_type: "subagent", description: "Review the release", state: "waiting" },
  };
  const previous = Array.from({ length: 60 }, (_, i) => row(`a${i + 1}`, `Previous result ${i + 1} ${"check the result ".repeat(35)}`));
  const messageText = Array.from({ length: 16 }, (_, i) => `Review request ${i + 1}`).join("\n\n");
  let polls = 0;
  await page.route("**/api/rows?*", (route) => {
    const agent = new URL(route.request().url()).searchParams.get("agent");
    if (agent !== "chair~sub~child-cut") return route.fulfill({ json: rows([batch]) });
    const message = row("u100", messageText, "user");
    const messages = [...previous, message];
    if (++polls > 1) messages.push({ ...row("r101", "Done", "result"), ends_turn: true });
    return route.fulfill({ json: rows(messages) });
  });
  await page.clock.install();
  await open(page);
  await page.getByTestId("subagent-row").getByRole("button").click();
  const chat = page.getByTestId("subagent-chat");
  const transcript = chat.getByTestId("pane-transcript");
  const message = transcript.locator('[data-id="u100"]');
  await message.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(message).toContainText("Review request 16");
  await page.clock.runFor(3000);
  await expect(transcript.locator('[data-id="r101"]')).toContainText("Turn finished");
  // Let the refresh's scroll-to-bottom settle before requiring a virtual row to unmount.
  await expect.poll(async () => {
    await transcript.evaluate((el) => { el.scrollTop = 0; });
    return message.count();
  }, { timeout: 2000 }).toBe(0);
  await transcript.evaluate((el) => { el.scrollTop = el.scrollHeight; });
  await expect(message).toContainText("Review request 16", { timeout: 2000 });
});
