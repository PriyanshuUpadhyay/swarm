import { expect, test, type Page } from "@playwright/test";
import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Meter } from "../src/core/settings-types";
import { open, settings } from "./settings-helpers";
import { captureThemes, changeRow, openChanges } from "./ux-helpers";

async function addReadmeComment(tab: Page, text: string) {
  await openChanges(tab, "README.md");
  const body = tab.getByTestId("diff-body");
  await body.press("j");
  await body.press("Enter");
  await tab.getByTestId("comment-input").fill(text);
  await tab.getByTestId("comment-add").click();
}

test("refreshing the same diff preserves keyboard selection without opening a comment", async ({ page: tab }) => {
  await openChanges(tab, "README.md");
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
  let fullReads = 0;
  await tab.route("**/api/changes/diff?**", async (route) => {
    if (new URL(route.request().url()).searchParams.get("full") !== "1") return route.continue();
    fullReads++;
    await route.fulfill({ status: 413, json: { error: "The full diff is over the cap." } });
  });
  await openChanges(tab, "big.txt");
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

test("Changes selects the next file when a full diff leaves the list", async ({ page: tab }) => {
  let removed = false;
  await tab.route("**/api/changes?**", async (route) => {
    const response = await route.fetch();
    const data = await response.json();
    const paths = removed ? ["README.md", "notes.md"] : ["README.md", "big.txt", "notes.md"];
    await route.fulfill({ response, json: {
      ...data, files: paths.map((path) => data.files.find((file: { path: string }) => file.path === path)),
    } });
  });

  try {
    await openChanges(tab, "big.txt");
    const cut = tab.getByTestId("diff-capped");
    await cut.getByRole("button", { name: "Show all", exact: true }).click();
    await expect(cut).toBeHidden();

    removed = true;
    await tab.evaluate(() => dispatchEvent(new Event("focus")));
    await expect(changeRow(tab, "big.txt")).toHaveCount(0, { timeout: 2_000 });
    await expect(tab.getByTestId("diff-path")).toHaveText("notes.md", { timeout: 2_000 });
    await expect(cut).toBeHidden();
  } finally {
    await tab.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("Show all can retry a transient full diff failure", async ({ page: tab }) => {
  let fullReads = 0;
  await tab.route("**/api/changes/diff?**", (route) => {
    const full = new URL(route.request().url()).searchParams.get("full") === "1";
    if (full && ++fullReads === 1) return route.fulfill({ status: 503, json: { error: "Temporarily unavailable" } });
    return route.continue();
  });

  await openChanges(tab, "big.txt");
  const cut = tab.getByTestId("diff-capped");
  const show = cut.getByRole("button", { name: "Show all", exact: true });
  await show.click();
  await expect(cut.getByRole("status")).toBeVisible();
  expect(fullReads).toBe(1);
  await show.click({ timeout: 2000 });
  await expect(cut).toBeHidden();
});

test("Show all keeps an unfinished range comment on the same lines", async ({ page: tab }) => {
  let fullReads = 0;
  await tab.route("**/api/changes/diff?**", async (route) => {
    const full = new URL(route.request().url()).searchParams.get("full") === "1";
    if (!full || ++fullReads !== 1) return route.continue();
    const response = await route.fetch();
    const data = await response.json();
    const first = data.display.lines.find((line: { text: string }) => line.text === "generated line 1");
    first.text = "changed line 1";
    first.quote = "+changed line 1";
    data.patch = data.patch.replace("+generated line 1\n", "+changed line 1\n");
    await route.fulfill({ response, json: data });
  });

  await openChanges(tab, "big.txt");
  const diff = tab.getByTestId("diff-body");
  const first = diff.locator('[data-gutter] > [data-column-number="1"]');
  await first.click();
  const input = tab.getByTestId("comment-input");
  await input.fill("Keep this note for line one.");
  const cut = tab.getByTestId("diff-capped");
  const show = cut.getByRole("button", { name: "Show all", exact: true });
  await show.click();
  await expect(cut.getByRole("status")).toBeVisible({ timeout: 2000 });
  await expect(input).toHaveValue("Keep this note for line one.");

  await show.click();
  await expect(cut).toBeHidden();
  await expect(input).toHaveValue("Keep this note for line one.");
  await expect(diff.locator('[data-content] > [data-line][data-selected-line]')).toHaveText("generated line 1");
});

test("removing a saved Changes comment asks first", async ({ page: tab }) => {
  await addReadmeComment(tab, "Keep this until I confirm.");

  const comment = tab.getByTestId("comment-card");
  const remove = comment.getByRole("button", { name: "Remove comment" });
  const dialog = tab.getByRole("dialog", { name: "Remove this comment?" });
  await remove.click();
  await expect(dialog).toBeVisible({ timeout: 2000 });
  await captureThemes(tab, dialog, "remove-comment");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(comment).toContainText("Keep this until I confirm.");

  await remove.click();
  await dialog.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(comment).toHaveCount(0);
});

test("removing a Changes tray comment asks first", async ({ page: tab }) => {
  await addReadmeComment(tab, "Confirm removal from the tray.");

  const tray = tab.getByTestId("drafts");
  const remove = tray.getByRole("button", { name: "Remove comment" });
  const dialog = tab.getByRole("dialog", { name: "Remove this comment?" });
  await remove.click();
  await expect(dialog).toBeVisible({ timeout: 2_000 });
  await captureThemes(tab, dialog, "remove-tray-comment");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(tray).toContainText("Confirm removal from the tray.");

  await remove.click();
  await dialog.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(tray).toHaveCount(0);
});

test("Home and End pick the first or last selectable diff line", async ({ page: tab }) => {
  await openChanges(tab, "README.md");
  await expect(tab.getByTestId("diff-path")).toHaveText("README.md");
  const diff = tab.getByTestId("diff-body");
  const selection = diff.locator('[data-content] > [data-line][data-selected-line]');
  await diff.press("End");
  await expect(selection).toHaveText("A test repository, changed.", { timeout: 2000 });
  await diff.press("Home");
  await expect(selection).toHaveText("# Project");
});

test("an oversized Changes range keeps the note and asks for fewer lines", async ({ page: tab }) => {
  await tab.route("**/api/changes/diff?**", (route) => {
    if (new URL(route.request().url()).searchParams.get("full") !== "1") return route.continue();
    // The two quoted lines together exceed the 64 KiB request limit.
    const lines = ["x", "y"].map((ch, i) => ({ kind: "add", text: ch.repeat(36_000), new: i + 1 }));
    return route.fulfill({ json: { path: "big.txt", layer: "untracked", patch: "full patch", display: { lines }, added: 2, removed: 0 } });
  });

  await openChanges(tab, "big.txt");
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

test("Show all does not open a comment for a keyboard selection", async ({ page: tab }) => {
  await openChanges(tab, "big.txt");
  const diff = tab.getByTestId("diff-body");
  const input = tab.getByTestId("comment-input");
  await diff.press("j");
  await expect(input).toHaveCount(0);

  const cut = tab.getByTestId("diff-capped");
  await cut.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(cut).toBeHidden({ timeout: 2_000 });
  await expect(input).toHaveCount(0, { timeout: 2_000 });
});

test("Show all keeps a selection on a line cut by the character limit", async ({ page: tab }) => {
  const name = "round4-long-diff-line.txt";
  const file = join(process.env.SWARM_E2E_GIT_DIR!, name);
  writeFileSync(file, "a".repeat(14_000) + "\n", { flag: "wx" });

  try {
    await openChanges(tab, name);
    await tab.getByTestId("diff-body").press("End");
    const cut = tab.getByTestId("diff-capped");
    await cut.getByRole("button", { name: "Show all", exact: true }).click();
    await expect(cut).toBeHidden({ timeout: 2_000 });
  } finally {
    rmSync(file);
  }
});

test("a refused expansion keeps the draft preview through a focus refresh", async ({ page: tab }) => {
  let replace = false;
  await tab.route("**/api/changes/diff?**", async (route) => {
    const response = await route.fetch();
    if (!replace || new URL(route.request().url()).searchParams.get("path") !== "big.txt") return route.fulfill({ response });

    const data = await response.json();
    const first = data.display.lines.find((line: { text: string }) => line.text === "generated line 1");
    first.text = "replaced line 1";
    first.quote = "+replaced line 1";
    data.patch = data.patch.replace("+generated line 1\n", "+replaced line 1\n");
    await route.fulfill({ response, json: data });
  });

  try {
    await openChanges(tab, "big.txt");
    const diff = tab.getByTestId("diff-body");
    await diff.locator('[data-gutter] > [data-column-number="1"]').click();
    const input = tab.getByTestId("comment-input");
    await input.fill("Do not move this draft.");
    replace = true;
    const cut = tab.getByTestId("diff-capped");
    await cut.getByRole("button", { name: "Show all", exact: true }).click();
    await expect(cut.getByRole("status")).toHaveText("The selected lines changed. The preview stays open.");

    const readsPreview = tab.waitForRequest((req) => {
      const url = new URL(req.url());
      return url.pathname === "/api/changes/diff" && !url.searchParams.has("full");
    }, { timeout: 1_000 }).then(() => true, () => false);
    const refreshed = tab.waitForResponse((res) => new URL(res.url()).pathname === "/api/changes");
    await tab.evaluate(() => dispatchEvent(new Event("focus")));
    expect((await refreshed).ok()).toBe(true);
    expect(await readsPreview).toBe(false);
    await expect(diff).toContainText("generated line 1");
    await expect(input).toHaveValue("Do not move this draft.");

    await tab.route("**/api/changes?**", async (route) => {
      const response = await route.fetch();
      const data = await response.json();
      data.files = data.files.filter((file: { path: string }) => file.path !== "big.txt");
      await route.fulfill({ response, json: data });
    });
    await tab.evaluate(() => dispatchEvent(new Event("focus")));
    await expect(changeRow(tab, "big.txt")).toHaveCount(0, { timeout: 2_000 });
    await expect(tab.getByTestId("diff-path")).not.toHaveText("big.txt", { timeout: 2_000 });
    await expect(input).toHaveCount(0);
  } finally {
    await tab.unrouteAll({ behavior: "ignoreErrors" });
  }
});

test("unfinished comment text survives Escape and reload", async ({ page: tab }) => {
  await openChanges(tab, "README.md");
  const draft = "Keep this unfinished comment.";
  const body = tab.getByTestId("diff-body");
  const input = tab.getByTestId("comment-input");
  const openEditor = async () => {
    await body.press("j");
    await body.press("Enter");
  };

  const gutter = body.locator('[data-gutter] > [data-column-number="1"]').first();
  await gutter.click();
  await input.fill(draft);
  await gutter.click();
  await expect(input).toHaveCount(0);
  await gutter.click();
  await expect(input).toHaveValue(draft, { timeout: 2_000 });

  await input.press("Escape");
  await expect(input).toHaveCount(0);
  await openEditor();
  await expect(input).toHaveValue(draft, { timeout: 2_000 });

  await tab.reload();
  await changeRow(tab, "README.md").click();
  await expect(input).toHaveCount(0);
  await openEditor();
  await expect(input).toHaveValue(draft, { timeout: 2_000 });
  await tab.getByRole("button", { name: "Cancel", exact: true }).click();
});

test("a Changes refresh during Add reports that the comment was not added", async ({ page: tab }) => {
  let resume!: () => void;
  const held = new Promise<void>((done) => { resume = done; });
  await tab.route("**/api/changes/comment-range", async (route) => {
    await held;
    await route.continue();
  });

  try {
    await Promise.all([
      tab.waitForRequest("**/api/changes/comment-range"),
      addReadmeComment(tab, "Add while the diff refreshes."),
    ]);
    await tab.route("**/api/changes/diff?**", async (route) => {
      const response = await route.fetch();
      await route.fulfill({ response, json: { ...await response.json(), added: 700 } });
    });

    await tab.evaluate(() => dispatchEvent(new Event("focus")));
    await expect(tab.getByTestId("diff-counts")).toContainText("+700");
    resume();
    await expect(tab.getByText("Could not add the comment. The diff changed. Try again.", { exact: true })).toBeVisible({ timeout: 2_000 });
    await expect(tab.getByTestId("comment-input")).toHaveValue("Add while the diff refreshes.");
  } finally {
    resume();
    await tab.unrouteAll({ behavior: "ignoreErrors" });
  }
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
