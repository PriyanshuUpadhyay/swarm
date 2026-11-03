import { expect, test, type Page } from "@playwright/test";
import { resolve } from "node:path";

// The Skills page (ADR 0111) on this repo's kit, which the e2e home links at ~/.swarm/kit.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const SHOTS = resolve(import.meta.dirname, "../../tmp/skills-shots");

/** A screenshot for the result, only when E2E_SHOTS is set and in WebKit. */
async function shoot(page: Page, name: string, browser: string) {
  if (process.env.E2E_SHOTS && browser === "webkit") await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

async function open(tab: Page) {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

test("the Blocks page shows the run tree of a starting plan beside the library, and a library pick finds the block in the tree", async ({ page }, info) => {
  const errors = await open(page);
  await page.getByTestId("skills-button").click();
  const dialog = page.getByTestId("skills");
  const tree = page.getByTestId("skills-tree");
  await expect(dialog).toBeVisible();
  await expect(page.getByTestId("skills-start")).toHaveValue("implement");
  await expect(page.getByTestId("skills-front").locator("[data-block]")).toHaveText(["planner", "plan-chart", "plan-review"]);
  await expect(tree.locator('[aria-level="1"][aria-expanded]')).toHaveCount(8);
  await shoot(page, "blocks", info.project.name);

  // A library pick opens the detail in the list and marks the block in the tree.
  await page.getByTestId("skills-filter").fill("change-review");
  await page.getByTestId("skills-library").locator('[data-block="change-review"]').click();
  await expect(page.getByTestId("skills-used-by")).toContainText("implement 06-review");
  await expect(page.getByTestId("skills-where")).toContainText("In this plan at 06-review");
  await expect(tree.locator('[data-block="change-review"][data-hit]')).toHaveCount(1);

  // A block outside the plan says so.
  await page.getByTestId("skills-start").selectOption("research");
  await page.getByTestId("skills-library").locator('[data-block="change-review"]').click();
  await expect(page.getByTestId("skills-where")).toContainText("Not in this plan");
  await page.getByTestId("skills-filter").fill("");

  // A family node names only the parts the step loads, and opens to all its parts.
  const family = tree.locator('[data-family="sources"]').first();
  await expect(family).toContainText("4 of 5 load here");
  await family.locator("button").first().click();
  await expect(family.locator('[role="group"] [data-block]')).toHaveCount(5);
  await expect(family.locator('[data-block="link-check"]')).toContainText("not loaded here");

  // The tree follows the tree view keys: Down moves, Left closes a step.
  await page.getByTestId("skills-front").focus();
  await page.keyboard.press("ArrowDown");
  await expect(tree.locator('[data-key^="s:01"]')).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await expect(tree.locator('[data-key^="s:01"]')).toHaveAttribute("aria-expanded", "false");

  await page.getByTestId("skills-start").selectOption("blank");
  await expect(page.getByTestId("skills-summary")).toContainText("Blank plan");
  await page.getByTestId("skills-start").selectOption("implement");
  await tree.locator('[data-block="prove-it-works"]').first().click();
  await expect(page.getByTestId("skills-detail")).toBeVisible();
  await shoot(page, "blocks-implement", info.project.name);
  await page.emulateMedia({ colorScheme: "dark" });
  await shoot(page, "blocks-dark", info.project.name);
  expect(errors).toEqual([]);
});

test("the Composer shows live text and graph, and Copy waits for Python to pass the exact text", async ({ page }, info) => {
  const errors = await open(page);
  await page.getByTestId("skills-button").click();
  await page.getByTestId("skills-view-composer").click();
  const copy = page.getByTestId("draft-copy");
  const text = page.getByTestId("draft-text");
  const status = page.getByTestId("draft-status");

  await page.getByTestId("draft-from").selectOption("explain");
  await expect(text).toContainText("## Preset\n\nStop: chat\n");
  await expect(status).toHaveText("Python passed this text.");
  await expect(copy).toBeEnabled();
  await expect(page.getByTestId("draft-graph").locator("[data-step]")).toHaveCount(2);

  // A pipe in a cell makes a sixth cell; Copy turns off at once and Python's defect shows.
  const holds = page.getByTestId("draft-row").first().locator('[data-col="Holds"]');
  const before = await holds.inputValue();
  await holds.fill(`${before} | more`);
  await expect(copy).toBeDisabled();
  await expect(text).toContainText(`${before} | more`);
  await expect(status).toContainText("row has 6 cells, not 5");
  await expect(copy).toBeDisabled();

  await holds.fill(before);
  await page.getByTestId("draft-add").click();
  const row = page.getByTestId("draft-row").last();
  await row.locator('[data-col="File"]').fill("03-check");
  await row.locator('[data-col="Needs"]').fill("reply");
  await row.locator('[data-col="Holds"]').fill("the checked answer");
  await row.locator('[data-col="Block"]').fill("voice-reply");
  await row.locator('[data-col="Why"]').fill("the owner reads it once more");
  await expect(status).toHaveText("Python passed this text.");
  await expect(page.getByTestId("draft-graph").locator("[data-step]")).toHaveCount(3);
  // The Composer graph only shows the draft, so its cards are not buttons.
  await expect(page.getByTestId("draft-graph").getByRole("button")).toHaveCount(0);
  await copy.click();
  await shoot(page, "composer", info.project.name);
  // A new start drops the edits at once, and Undo brings them back.
  await page.getByTestId("draft-from").selectOption("research");
  await expect(text).not.toContainText("03-check");
  await page.getByTestId("draft-undo").click();
  await expect(page.getByTestId("draft-undo")).toHaveCount(0);
  await expect(page.getByTestId("draft-from")).toHaveValue("explain");
  await expect(text).toContainText("03-check");
  expect(errors).toEqual([]);
});

test("a new skill copies a whole SKILL.md", async ({ page }) => {
  const errors = await open(page);
  await page.getByTestId("skills-button").click();
  await page.getByTestId("skills-view-composer").click();
  await page.getByTestId("draft-from").selectOption("");
  await page.getByTestId("draft-name").fill("triage");
  await page.getByTestId("draft-description").fill("Use when a bug report arrives.");
  const row = page.getByTestId("draft-row").first();
  await row.locator('[data-col="Holds"]').fill("the goal and the done-when");
  await row.locator('[data-col="Why"]').fill("the fix rests on them");
  await expect(page.getByTestId("draft-status")).toContainText("01-frame");
  await row.locator('[data-col="Block"]').fill("requirements, voice-reply");
  await expect(page.getByTestId("draft-text")).toContainText('---\nname: triage\ndescription: "Use when a bug report arrives."\n---\n\n# triage\n\n## Preset');
  await expect(page.getByTestId("draft-status")).toHaveText("Python passed this text.");
  await expect(page.getByTestId("draft-copy")).toBeEnabled();
  expect(errors).toEqual([]);
});
