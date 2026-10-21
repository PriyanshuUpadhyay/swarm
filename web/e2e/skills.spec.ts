import { expect, test, type Page } from "@playwright/test";
import { resolve } from "node:path";

// The Skills page (ADR 0111) on this repo's kit, which the e2e home links at ~/.swarm/kit.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const SHOTS = resolve(import.meta.dirname, "../../tmp/skills-shots");

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

/** Edges that pass through a card that is not one of their ends, as the vitest counts them. */
function crossings({ boxes, edges }: Awaited<ReturnType<typeof drawn>>) {
  const hits: string[] = [];
  for (const e of edges)
    for (const b of boxes) {
      if (b.file === e.from || b.file === e.to) continue;
      for (let i = 1; i < e.points.length; i++) {
        const [p, q] = [e.points[i - 1], e.points[i]];
        for (let t = 0; t <= 1; t += 1 / 60) {
          const [x, y] = [p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t];
          if (x > b.x + 1 && x < b.x + b.w - 1 && y > b.y + 1 && y < b.y + b.h - 1) hits.push(`${e.from}->${e.to} via ${b.file}`);
        }
      }
    }
  return [...new Set(hits)];
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
  if (info.project.name === "chromium") await page.screenshot({ path: `${SHOTS}/catalog.png` });
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
    const presets = page.getByRole("tablist", { name: "Default skills" }).getByRole("tab");
    await expect(presets.first()).toBeVisible();
    for (const preset of await presets.all()) {
      await preset.click();
      await expect(page.locator("[data-edge]").first()).toBeAttached();
      expect(crossings(await drawn(page)), await preset.innerText()).toEqual([]);
    }
    await page.locator('[data-preset="implement"]').click();
    await page.locator('[data-step="05-build"]').click();
    await expect(page.getByTestId("skills-step")).toContainText("checked-units");
    if (info.project.name === "chromium") await page.screenshot({ path: `${SHOTS}/graph-${name}.png` });
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
  await copy.click();
  if (chromium) {
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await text.textContent());
    await page.screenshot({ path: `${SHOTS}/composer.png` });
  }
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
  await expect(page.getByTestId("draft-text")).toContainText("---\nname: triage\ndescription: Use when a bug report arrives.\n---\n\n# triage\n\n## Preset");
  await expect(page.getByTestId("draft-status")).toHaveText("Python passed this text.");
  await expect(page.getByTestId("draft-copy")).toBeEnabled();
  expect(errors).toEqual([]);
});
