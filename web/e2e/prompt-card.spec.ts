import { expect, test, type Page } from "@playwright/test";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { PANE_PROJECTS } from "./serve";

const screens = resolve(import.meta.dirname, "../../tests/fixtures/screens");
const idle = readFileSync(join(screens, "claude-idle.txt"), "utf8");
const waiting = readFileSync(join(screens, "claude-waiting.txt"), "utf8");

async function openChat(tab: Page, session: string) {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
}

test("a long error token wraps and keeps Retry and Dismiss inside the banner", async ({ page: tab }, info) => {
  const home = process.env.SWARM_E2E_HOME!;
  const error = `failed-${"unbroken".repeat(100)}`;
  await openChat(tab, process.env.SWARM_E2E_LIVE!);
  await tab.setViewportSize({ width: 900, height: 900 });
  const composer = tab.getByTestId("composer");
  await expect(composer).toBeEnabled();
  try {
    writeFileSync(join(home, "fail"), error);
    await composer.fill("show the send error");
    await composer.press("Enter");
    const banner = tab.getByTestId("chat-banner");
    await expect(banner).toContainText(error);
    expect(await banner.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
    const box = (await banner.boundingBox())!;
    const input = (await tab.locator(".chat-input").boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(input.x + input.width + 1);
    for (const name of ["Retry", "Dismiss"]) {
      const button = banner.getByRole("button", { name });
      await expect(button).toBeVisible();
      const bounds = (await button.boundingBox())!;
      expect(bounds.x).toBeGreaterThanOrEqual(box.x);
      expect(bounds.x + bounds.width).toBeLessThanOrEqual(box.x + box.width + 1);
    }
    await tab.screenshot({ path: info.outputPath("long-error.png") });
    await banner.getByRole("button", { name: "Dismiss" }).click();
    await expect(banner).toBeHidden();
  } finally {
    rmSync(join(home, "fail"), { force: true });
    await composer.fill("");
  }
});

test("a long question path wraps inside its labelled group", async ({ page: tab }, info) => {
  const screen = join(process.env.SWARM_E2E_HOME!, "screen");
  const path = `/tmp/${"longpath".repeat(100)}`;
  writeFileSync(screen, idle);
  await openChat(tab, process.env.SWARM_E2E_LIVE!);
  await tab.setViewportSize({ width: 900, height: 900 });
  const card = tab.getByTestId("prompt-card");
  await expect(card).toBeHidden();
  try {
    writeFileSync(screen, waiting.replace("Do you want to proceed?", path));
    await expect(card).toContainText(path);
    const question = card.getByText(path);
    expect(await question.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
    expect(await card.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
    const box = (await card.boundingBox())!;
    const input = (await tab.locator(".chat-input").boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(input.x + input.width + 1);
    await expect(card).toHaveRole("group");
    await expect(card).toHaveAccessibleName(new RegExp(`^The chair is waiting:.*${path}`, "s"));
    await tab.screenshot({ path: info.outputPath("long-question.png") });
  } finally {
    writeFileSync(screen, idle);
    await expect(card).toBeHidden();
  }
});

test("a child question updates one mounted polite status without taking focus", async ({ page: tab }, info) => {
  test.skip(!PANE_PROJECTS.includes(info.project.name), "The browser needs a child pane fixture.");
  const home = process.env.SWARM_E2E_HOME!;
  const screen = join(home, `screen-fixture-${info.project.name}-reviewer`);
  writeFileSync(screen, idle);
  await openChat(tab, process.env[`SWARM_E2E_PANES_${info.project.name}`]!);
  const reviewer = tab.locator('[data-testid="pane"][data-pane="reviewer"]');
  const status = reviewer.locator(".chat-input").getByRole("status");
  const caption = "The agent is asking a question. Answer it first.";
  await expect(status).toHaveCount(1);
  await expect(status).toBeEmpty();
  const mounted = (await status.elementHandle())!;
  await status.evaluate((el, caption) => {
    const target = el as HTMLElement & { announcements: string[] };
    target.announcements = [];
    new MutationObserver(() => {
      if (target.textContent === caption) target.announcements.push(target.textContent);
    }).observe(target, { childList: true, subtree: true, characterData: true });
  }, caption);
  const composer = tab.locator(".strip-chat").getByTestId("composer");
  await composer.focus();
  try {
    for (let i = 1; i <= 2; i++) {
      writeFileSync(screen, waiting);
      await expect(status).toHaveText(caption);
      await expect(tab.getByRole("status").filter({ hasText: caption })).toHaveCount(1);
      await expect(composer).toBeFocused();
      await expect(tab.getByTestId("asking").getByRole("status")).toHaveCount(0);
      await expect(status.getByRole("button")).toHaveCount(0);
      expect(await mounted.evaluate((el) => el.isConnected)).toBe(true);
      await expect.poll(() => status.evaluate((el) => (el as HTMLElement & { announcements: string[] }).announcements.length)).toBe(i);
      await tab.screenshot({ path: info.outputPath(`child-question-${i}.png`) });
      writeFileSync(screen, idle);
      await expect(status).toBeEmpty();
    }
  } finally {
    writeFileSync(screen, idle);
  }
});
