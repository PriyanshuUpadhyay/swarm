import { expect, test } from "@playwright/test";
import { copyFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { home, open } from "./settings-helpers";

test("the working glyph loops at 1.8 seconds and stays still with reduced motion", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  const screen = join(home(), "screen-fixture-chair");
  copyFileSync(resolve(import.meta.dirname, "../../tests/fixtures/screens/claude-working.txt"), screen);
  try {
    await open(page);
    const chat = page.getByTestId("sidebar").locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_LIVE}"]`);
    const dots = chat.locator('.swarm-glyph[data-form="working"] .swarm-glyph-dot:not(:first-child)');
    await expect(dots.first()).toBeVisible();
    const styles = (property: "animationDuration" | "animationName") =>
      dots.evaluateAll((els, key) => [...new Set(els.map((el) => getComputedStyle(el)[key]))], property);
    await expect.poll(() => styles("animationDuration")).toEqual(["1.8s"]);

    await page.emulateMedia({ reducedMotion: "reduce" });
    await expect.poll(() => styles("animationName")).toEqual(["none"]);
  } finally {
    rmSync(screen, { force: true });
  }
});
