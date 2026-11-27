import { expect, test } from "@playwright/test";
import type { Meter } from "../src/core/settings-types";
import { open, settings } from "./settings-helpers";
import { captureThemes, open as openChat } from "./ux-helpers";

test("refreshing the same diff preserves keyboard selection without opening a comment", async ({ page: tab }) => {
  await openChat(tab, process.env.SWARM_E2E_GIT!);
  await tab.getByTestId("show-changes").click();
  await tab.locator('[data-testid="change-row"][data-path="README.md"]').click();
  await expect(tab.getByTestId("diff-path")).toHaveText("README.md");
  const body = tab.getByTestId("diff-body");
  const picked = tab.locator('[data-testid="diff-body"] [data-content] > [data-line][data-selected-line]');
  await body.focus();
  await tab.keyboard.press("j");
  await expect(picked).toHaveCount(1);
  await expect(body).toBeFocused();
  let added = 700;
  await tab.route("**/api/changes/diff?**", async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, json: { ...await response.json(), added } });
  });
  const refresh = async () => {
    added++;
    await tab.evaluate(() => dispatchEvent(new Event("focus")));
    await expect(tab.getByTestId("diff-counts")).toContainText(`+${added}`);
  };

  await refresh();
  await expect(body).toBeFocused();
  await expect(tab.getByTestId("comment-input")).toHaveCount(0);
  await expect(picked).toHaveCount(1);
  await tab.keyboard.press("Escape");
  await refresh();
  await expect(picked).toHaveCount(0);
});

test("focus refresh resumes after Show all is refused without repeating the full read", async ({ page: tab }) => {
  await openChat(tab, process.env.SWARM_E2E_GIT!);
  let fullReads = 0;
  await tab.route("**/api/changes/diff?**", async (route) => {
    if (new URL(route.request().url()).searchParams.get("full") !== "1") return route.continue();
    fullReads++;
    await route.fulfill({ status: 413, json: { error: "The full diff is over the cap." } });
  });
  await tab.getByTestId("show-changes").click();
  await tab.locator('[data-testid="change-row"][data-path="big.txt"]').click();
  const cut = tab.getByTestId("diff-capped");
  const show = cut.getByRole("button", { name: "Show all", exact: true });
  await show.click();
  await expect(cut).toContainText("The full diff is over the cap.");

  const refresh = tab.waitForResponse((res) => new URL(res.url()).pathname === "/api/changes", { timeout: 2500 });
  await tab.evaluate(() => dispatchEvent(new Event("focus")));
  expect((await refresh).ok()).toBe(true);
  await expect(cut).toContainText("The full diff is over the cap.");
  await expect(show).toHaveCount(0);
  expect(fullReads).toBe(1);
});

test("Changes updates its file list without replacing an open full diff", async ({ page: tab }) => {
  await openChat(tab, process.env.SWARM_E2E_GIT!);
  await tab.getByTestId("show-changes").click();
  await tab.locator('[data-testid="change-row"][data-path="big.txt"]').click();
  const cut = tab.getByTestId("diff-capped");
  await cut.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(cut).toBeHidden();

  await tab.route("**/api/changes?**", async (route) => {
    const response = await route.fetch();
    const data = await response.json();
    data.files = [...data.files.filter((file: { path: string }) => file.path !== "big.txt"), { path: "new-change.txt", status: "?", layer: "untracked", added: 1, removed: 0 }];
    await route.fulfill({ response, json: data });
  });
  await tab.evaluate(() => dispatchEvent(new Event("focus")));
  await expect(tab.locator('[data-testid="change-row"][data-path="new-change.txt"]')).toBeVisible({ timeout: 2000 });
  await expect(tab.getByTestId("diff-path")).toHaveText("big.txt");
  await expect(cut).toBeHidden();
});

test("Show all can retry a transient full diff failure", async ({ page: tab }) => {
  await openChat(tab, process.env.SWARM_E2E_GIT!);
  let fullReads = 0;
  await tab.route("**/api/changes/diff?**", (route) => {
    const full = new URL(route.request().url()).searchParams.get("full") === "1";
    if (full && ++fullReads === 1) return route.fulfill({ status: 503, json: { error: "Temporarily unavailable" } });
    return route.continue();
  });
  await tab.getByTestId("show-changes").click();
  await tab.locator('[data-testid="change-row"][data-path="big.txt"]').click();
  const cut = tab.getByTestId("diff-capped");
  const show = cut.getByRole("button", { name: "Show all", exact: true });
  await show.click();
  await expect(cut.getByRole("status")).toBeVisible();
  expect(fullReads).toBe(1);
  await show.click({ timeout: 2000 });
  await expect(cut).toBeHidden();
});

test("an oversized Changes range keeps the note and asks for fewer lines", async ({ page: tab }) => {
  await openChat(tab, process.env.SWARM_E2E_GIT!);
  await tab.route("**/api/changes/diff?**", (route) => {
    if (new URL(route.request().url()).searchParams.get("full") !== "1") return route.continue();
    const lines = ["x", "y"].map((ch, i) => ({ kind: "add", text: ch.repeat(36_000), new: i + 1 }));
    return route.fulfill({ json: { path: "big.txt", layer: "untracked", patch: "full patch", display: { lines }, added: 2, removed: 0 } });
  });
  await tab.getByTestId("show-changes").click();
  await tab.locator('[data-testid="change-row"][data-path="big.txt"]').click();
  await tab.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(tab.getByTestId("diff-capped")).toBeHidden();
  const diff = tab.getByTestId("diff-body");
  await diff.locator('[data-gutter] > [data-column-number]').first().click();
  await diff.locator('[data-gutter] > [data-column-number]').last().click({ modifiers: ["Shift"] });
  const selection = diff.locator('[data-content] > [data-line][data-selected-line]');
  await expect(selection).toHaveCount(2);
  const input = tab.getByTestId("comment-input");
  await input.fill("Check these lines before shipping.");

  const rejection = tab.waitForResponse((response) => new URL(response.url()).pathname === "/api/changes/comment-range");
  await tab.getByTestId("comment-add").click();
  expect((await rejection).status()).toBe(413);
  await expect(tab.getByText("Could not add the comment. Select fewer lines.", { exact: true })).toBeVisible();
  await expect(input).toHaveValue("Check these lines before shipping.");
  await expect(selection).toHaveCount(2);
});

test("the 680 px new-chat composer uses the display face only for its question", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  await page.getByTestId("home-button").click();
  const composer = page.getByTestId("new-chat-composer");
  const heading = composer.getByRole("heading", { name: "What should we work on?" });
  await expect(composer).toHaveCSS("width", "680px");
  await expect(heading).toHaveCSS("font-family", /Instrument Serif/);
  await expect(heading).toHaveCSS("font-size", "44px");
  await expect(heading).toHaveCSS("line-height", "52px");
  const prompt = composer.getByTestId("new-chat-prompt");
  await expect(prompt).not.toHaveCSS("font-family", /Instrument Serif/);
  await captureThemes(page, composer, "new-chat");
  await prompt.fill("Review the failing check.");
  await expect(composer.getByTestId("new-chat-start")).toBeEnabled();
});

test("the existing destructive confirmation is outlined while Save stays primary", async ({ page }) => {
  await open(page);
  const profiles = await settings(page, "profiles");
  await profiles.locator('[data-testid="profile-row"][data-name="chat"] [data-action="edit"]').click();
  await expect(profiles.getByTestId("profile-save")).toHaveClass(/(?:^| )bg-brand(?: |$)/);
  await profiles.getByTestId("profiles-reset").click();
  const yes = profiles.getByTestId("settings-confirm-yes");
  await expect(yes).toHaveClass(/(?:^| )border-failed(?: |$)/);
  await expect(yes).toHaveClass(/(?:^| )text-failed(?: |$)/);
  await expect(yes).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await captureThemes(page, profiles, "settings-confirm");
  await profiles.getByRole("button", { name: "Cancel", exact: true }).click();
});

test("the usage percentage uses the same state color as the bar of what is left", async ({ page }) => {
  const left = [21, 20, 5];
  const meters: Meter[] = left.map((percent, i) => ({
    provider: "claude", account: "default", label: "default", window: ["5h", "7d", "30d"][i], window_minutes: null,
    used_pct: 100 - percent, reset_time_seconds: null, state: "fresh", reason: null, reason_code: null, as_of_seconds: null,
  }));
  await page.route("**/api/usage", (route) => route.fulfill({ json: { meters } }));

  await open(page);
  const accounts = (await settings(page, "accounts")).getByTestId("accounts-claude");

  for (const [i, tone] of ["foreground", "waiting", "failed"].entries()) {
    const row = accounts.getByTestId("meter").nth(i);
    const label = row.getByText(`${left[i]}%`, { exact: true });
    const bar = row.getByRole("meter");
    await expect(bar).toHaveAttribute("aria-valuenow", String(left[i]));
    await expect(label).toHaveClass(new RegExp(`(?:^| )text-${tone}(?: |$)`));
    await expect(bar.locator("span")).toHaveCSS("background-color", await label.evaluate((el) => getComputedStyle(el).color));
  }
  await captureThemes(page, accounts, "usage");
});

test("Skills uses R/S/P/F boxes and keeps details in the library", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Meta+k");
  await page.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill("Skills");
  await page.locator('[cmdk-item][data-value="action:skills"]').click();
  const dialog = page.getByTestId("skills");
  const legend = dialog.getByRole("complementary", { name: "Block library" });
  await expect(legend.locator("p [data-skill-kind]")).toHaveText(["R", "S", "P", "F"]);
  const library = dialog.getByTestId("skills-library");
  await dialog.getByTestId("skills-filter").fill("change-review");
  const row = library.locator('[data-block="change-review"]');
  await expect(row.locator("[data-skill-kind]")).toBeVisible();
  await row.click();
  await expect(library.getByTestId("skills-detail")).toBeVisible();
  await captureThemes(page, dialog, "skills");
});
