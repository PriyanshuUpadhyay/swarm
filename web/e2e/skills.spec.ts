import { expect, test, type Page } from "@playwright/test";
import { resolve } from "node:path";
import { crossings } from "../src/core/skill-crossings";

// The Skills page (ADR 0111) on this repo's kit, which the e2e home links at ~/.swarm/kit.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const SHOTS = resolve(import.meta.dirname, "../../tmp/skills-shots");

/** A screenshot for the result, only when E2E_SHOTS is set and in Chromium. */
async function shoot(page: Page, name: string, browser: string) {
  if (process.env.E2E_SHOTS && browser === "chromium") await page.screenshot({ path: `${SHOTS}/${name}.png` });
}

async function open(tab: Page) {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

/** Each box of the drawn graph, and each edge's points, in page pixels. */
async function drawn(tab: Page) {
  return tab.getByTestId("skill-graph").evaluate((graph) => {
    const boxes = [...graph.querySelectorAll<HTMLElement>("[data-step]")].map((el) => {
      const r = el.getBoundingClientRect();
      return { file: el.dataset.step!, x: r.x, y: r.y, w: r.width, h: r.height };
    });
    const svg = graph.querySelector("svg")!.getBoundingClientRect();
    const edges = [...graph.querySelectorAll<SVGPolylineElement>("[data-edge]")].map((el) => {
      const [from, to] = el.dataset.edge!.split(">");
      const points = [...el.points].map((p) => ({ x: p.x + svg.x, y: p.y + svg.y }));
      return { from, to, points };
    });
    return { boxes, edges };
  });
}

test("the Catalog shows each block's users, and an unused block says so", async ({ page }, info) => {
  const errors = await open(page);
  await page.getByTestId("skills-button").click();
  const dialog = page.getByTestId("skills");
  await expect(dialog).toBeVisible();
  await page.getByTestId("skills-filter").fill("change-review");
  await dialog.locator('[data-block="change-review"]').click();
  await expect(page.getByTestId("skills-used-by")).toContainText("implement · 06-review");
  await expect(page.getByTestId("skills-used-by")).toContainText("named");
  await page.getByTestId("skills-filter").fill("");
  const unused = dialog.locator("[data-block]").filter({ hasText: "unused" }).first();
  await unused.click();
  await expect(page.getByTestId("skills-unused")).toHaveText("not used by a default preset");
  await shoot(page, "catalog", info.project.name);
  expect(errors).toEqual([]);
});

for (const [name, width] of [
  ["wide", 1280],
  ["narrow", 760],
] as const)
  test(`the Skill graph draws every default skill with no edge through a card (${name})`, async ({ page }, info) => {
    await page.setViewportSize({ width, height: 900 });
    const errors = await open(page);
    await page.keyboard.press("Meta+k");
    await page.getByTestId("palette").getByText("Skills…").click();
    await page.getByTestId("skills-view-graph").click();
    const presets = page.getByRole("navigation", { name: "Default skills" }).locator("[data-preset]");
    await expect(presets.first()).toBeVisible();
    for (const preset of await presets.all()) {
      await preset.click();
      await expect(page.locator("[data-edge]").first()).toBeAttached();
      expect(crossings(await drawn(page)), await preset.innerText()).toEqual([]);
    }
    await page.locator('[data-preset="implement"]').click();
    await page.locator('[data-step="05-build"]').click();
    await expect(page.getByTestId("skills-step")).toContainText("checked-units");
    await shoot(page, `graph-${name}`, info.project.name);
    expect(errors).toEqual([]);
  });

test("the Composer shows live text and graph, and Copy waits for Python to pass the exact text", async ({ page, context }, info) => {
  const chromium = info.project.name === "chromium";
  if (chromium) await context.grantPermissions(["clipboard-read", "clipboard-write"]);
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
  if (chromium) expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await text.textContent());
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
  await expect(page.getByTestId("draft-status")).toHaveText("the last step 01-frame has no voice- block");
  await row.locator('[data-col="Block"]').fill("requirements, voice-reply");
  await expect(page.getByTestId("draft-text")).toContainText('---\nname: triage\ndescription: "Use when a bug report arrives."\n---\n\n# triage\n\n## Preset');
  await expect(page.getByTestId("draft-status")).toHaveText("Python passed this text.");
  await expect(page.getByTestId("draft-copy")).toBeEnabled();
  expect(errors).toEqual([]);
});
