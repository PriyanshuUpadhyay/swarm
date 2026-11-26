import { expect, test, type Locator, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import type { Meter } from "../src/core/settings-types";
import { open, settings } from "./settings-helpers";

async function capture(page: Page, target: Locator, name: string) {
  const dir = process.env.DESIGN_B_SHOTS;
  if (!dir) return;
  mkdirSync(dir, { recursive: true });
  for (const theme of ["dark", "light"]) {
    await page.evaluate((theme) => { document.documentElement.dataset.theme = theme; }, theme);
    await target.screenshot({ path: join(dir, `${name}-${theme}.png`) });
  }
}

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
  await capture(page, composer, "new-chat");
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
  await capture(page, profiles, "settings-confirm");
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
  await capture(page, accounts, "usage");
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
  await capture(page, dialog, "skills");
});
