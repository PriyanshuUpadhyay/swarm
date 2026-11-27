import { expect, test } from "@playwright/test";
import { bodyMode, row, rows, open, mockRows } from "./ux-helpers";
import fencePainter from "../../tests/fixtures/s6c/painter.json" with { type: "json" };
import type { Display, PatchInfo } from "../src/core/api";
import previewPatch from "./fixtures/s6a-preview.json" with { type: "json" };
import fullLines0 from "./fixtures/s6a-full-lines-0.json" with { type: "json" };
import fullLines1 from "./fixtures/s6a-full-lines-1.json" with { type: "json" };
import fullLines2 from "./fixtures/s6a-full-lines-2.json" with { type: "json" };
import fullSplit0 from "./fixtures/s6a-full-split-0.json" with { type: "json" };

/** The patch line number of the row that holds the current Find match, inside Pierre's shadow root. */
const currentLine = (tab: import("@playwright/test").Page) => tab.evaluate(() => {
  const range = [...(CSS.highlights.get("find-current") ?? [])][0] as Range | undefined;
  return range?.startContainer.parentElement?.closest("[data-line]")?.getAttribute("data-line") ?? null;
});

test("Find reaches repeated matches in a loaded full patch in Unified and Split views", async ({ page: tab }) => {
  const r = row("d1", "", "diff");
  r.diff = { id: "patch-output", path: "app.ts", patch: "@@ -1 +1 @@\n-old\n+preview", added: 1, removed: 1, omitted_lines: 900, shortened_lines: 0, full_lines: 1001, full_bytes: 20000 };
  await mockRows(tab, [r], 0, previewPatch as Record<string, PatchInfo>, { d1: [{ f: "diff.patch", p: "patch-output" }] });
  const patch = ["@@ -1,1000 +1,1000 @@", ...Array.from({ length: 1000 }, (_, i) => ` ${i === 100 || i === 900 ? "repeated needle" : `line ${i}`}`)].join("\n");
  await tab.route("**/api/tool-output?*", (route) => route.fulfill({ json: { text: patch, patch: { lines: fullLines0.concat(fullLines1, fullLines2), split: fullSplit0 } } }));
  await open(tab);
  await tab.getByRole("button", { name: "Show all", exact: true }).click();
  for (const mode of ["Unified", "Split"]) {
    await tab.getByRole("button", { name: mode, exact: true }).click();
    await tab.keyboard.press("Meta+f");
    await tab.getByTestId("find-input").fill("repeated needle");
    await expect(tab.getByTestId("find-count")).toHaveText("1 of 2");
    await expect(tab.getByTestId("diff").locator('[data-content] > [data-line="901"]').first()).toBeInViewport();
    await expect.poll(() => currentLine(tab)).toBe("901");
    await tab.getByTestId("find-input").press("Enter");
    await expect(tab.getByTestId("find-count")).toHaveText("2 of 2");
    await expect(tab.getByTestId("diff").locator('[data-content] > [data-line="101"]').first()).toBeInViewport();
    await expect.poll(() => currentLine(tab)).toBe("101");
    await tab.getByTestId("find-input").press("Escape");
  }
});

// Found when a Copy message click missed: the loading box drew the whole patch and moved the rows below.
test("a patch box keeps its height when Pierre loads", async ({ page: tab }) => {
  let release = () => {};
  const held = new Promise<void>((done) => (release = done));
  await tab.route("**/assets/pierre-patch-*.js", async (route) => {
    await held;
    await route.continue();
  });
  const tool = row("tool1", "", "diff");
  const lines = Array.from({ length: 40 }, (_, i) => ({ kind: "ctx" as const, text: `line ${i}`, old: i + 1, new: i + 1 }));
  tool.diff = { id: "held", path: "x.ts", patch: ["@@ -1,40 +1,40 @@", ...lines.map((l) => ` ${l.text}`)].join("\n"), added: 0, removed: 0, omitted_lines: 0, shortened_lines: 0, full_lines: 41, full_bytes: 400 };
  await tab.route("**/api/rows?*", route => route.fulfill({ json: rows([tool], 0, { held: {
    lines: [{ kind: "hunk", text: "@@ -1,40 +1,40 @@" }, ...lines],
    split: [{ h: 0 }, ...lines.map((_, i) => ({ l: i + 1, r: i + 1 }))],
  } as PatchInfo }) }));
  await open(tab);
  const card = tab.getByTestId("diff");
  const height = () => card.evaluate((el) => el.getBoundingClientRect().height);
  await expect(card.locator("pre.out")).toBeVisible();
  const loading = await height();
  release();
  await expect(card.locator('[data-content] > [data-line="1"]')).toBeVisible();
  await expect.poll(height).toBe(loading);
});

test("Split diff scrolls a long line sideways, and Wrap wraps it", async ({ page: tab }, info) => {
  await tab.addInitScript(() => {
    localStorage.setItem("swarm.diff.split", "1");
    localStorage.setItem("swarm.diff.wrap", "0");
  });
  const long = `const value = ${"word ".repeat(80)};`;
  const tool = row("tool1", "", "diff");
  tool.diff = { id: "long-diff", path: "x.ts", patch: `@@ -1 +1 @@\n-old\n+${long}\n`, added: 1, removed: 1, omitted_lines: 0, shortened_lines: 0, full_lines: 3, full_bytes: 500 };
  await tab.route("**/api/rows?*", route => route.fulfill({ json: rows([tool], 0, { "long-diff": {
    lines: [{ kind: "hunk", text: "@@ -1 +1 @@" }, { kind: "del", text: "old", old: 1 }, { kind: "add", text: long, new: 1 }],
    split: [{ h: 0 }, { l: 1, r: 2 }],
  } as PatchInfo }) }));
  await open(tab);
  const card = tab.getByTestId("diff");
  const patch = card.locator(".patch");
  const added = card.locator('code[data-additions] [data-content] > [data-line="1"]');
  const wrapButton = card.getByRole("button", { name: "Wrap", exact: true });
  const height = () => added.evaluate((el) => el.getBoundingClientRect().height);
  await expect(added).toHaveText(long);
  await expect(card.locator("pre[data-diff]")).toHaveAttribute("data-overflow", "scroll");
  await expect.poll(height).toBeLessThan(30);
  await tab.screenshot({ path: info.outputPath("split-scroll.png") });

  await wrapButton.click();
  await expect(wrapButton).toHaveAttribute("aria-pressed", "true");
  await expect(card.locator("pre[data-diff]")).toHaveAttribute("data-overflow", "wrap");
  await expect.poll(height).toBeGreaterThan(40);
  await expect.poll(() => patch.evaluate((el) => el.scrollHeight <= el.clientHeight + 1)).toBe(true);
  await tab.screenshot({ path: info.outputPath("split-wrap.png") });

  await card.getByRole("button", { name: "Unified", exact: true }).click();
  await expect(wrapButton).toHaveCount(0);
  await expect(card.locator("pre[data-diff]")).toHaveAttribute("data-overflow", "scroll");
});

// S6C-FENCES-V1 T2. Written only; no native clipboard and no browser run.
test("a fence and a tool share wire rows and layout while Copy keeps full text", async ({ page: tab }) => {
  await tab.addInitScript(() => {
    localStorage.setItem("swarm.diff.split", "0");
    const captured: string[] = [];
    Object.defineProperty(window, "s6cCopies", { value: captured });
    Object.defineProperty(navigator, "clipboard", { value: { writeText: async (text: string) => { captured.push(text); } } });
  });

  const fence = row("fence1", "raw Markdown deliberately differs from display");
  const tool = row("tool2", "", "diff");
  tool.diff = { id: "fence-tool", path: "x", patch: "@@ -1 +1 @@\n-tool-raw\n+tool-raw\n", added: 1, removed: 1, omitted_lines: 0, shortened_lines: 0, full_lines: 5, full_bytes: 48 };
  const page = rows([fence, tool], 0, { "fence-tool": fencePainter.patch as PatchInfo });
  page.display![fence.id] = structuredClone(fencePainter.display) as Display;

  page.display![fence.id].blocks.push({ type: "code", value: {
    label: "diff", text: "multi-file raw copy text", diff: {
      lines: [
        { kind: "meta", text: "--- a/old-a.ts" }, { kind: "meta", text: "+++ b/a.ts" },
        { kind: "hunk", text: "@@ -1,1 +1,1 @@" },
        { kind: "del", text: "old-a", old: 1 }, { kind: "add", text: "new-a", new: 1 },
        { kind: "meta", text: "--- a/old-b.ts" }, { kind: "meta", text: "+++ b/b.ts" },
        { kind: "hunk", text: "@@ -1,1 +1,1 @@" },
        { kind: "del", text: "old-b", old: 1 }, { kind: "add", text: "new-b", new: 1 },
      ],
      split: [{ h: 2 }, { l: 3, r: 4 }, { h: 7 }, { l: 8, r: 9 }],
    },
  } });
  page.body![fence.id] = bodyMode(page.display![fence.id]);

  await tab.route("**/api/rows?*", route => route.fulfill({ json: page }));
  await open(tab);

  const hunk = tab.locator('[data-id="fence1"] .md-code').nth(0);
  const prefix = tab.locator('[data-id="fence1"] .md-code').nth(1);
  const card = tab.getByTestId("diff");
  await expect(tab.locator('[data-id="fence1"] .md-code').nth(2).locator("[data-diffs-header] [data-title]")).toHaveText(["a.ts", "b.ts"]);
  const rowOf = (box: typeof card, type: string, side = "") => box.locator(`code${side} [data-content] > [data-line-type="${type}"]`);
  const numberOf = (box: typeof card, type: string, side = "") => box.locator(`code${side} [data-gutter] > [data-line-type="${type}"]`);
  for (const box of [hunk, card]) {
    await expect(box.locator(".patch")).toBeVisible();
    await expect(rowOf(box, "change-deletion")).toHaveText("<script>x</script>");
    await expect(rowOf(box, "change-addition")).toHaveText("<b>&new</b>");
    await expect(numberOf(box, "change-deletion")).toHaveText("1");
    await expect(numberOf(box, "change-addition")).toHaveText("1");
    await expect(box.locator("script, b")).toHaveCount(0);
  }

  const unified = hunk.getByRole("button", { name: "Unified", exact: true });
  const split = hunk.getByRole("button", { name: "Split", exact: true });
  const wrap = hunk.getByRole("button", { name: "Wrap", exact: true });
  await expect(unified).toHaveAttribute("aria-pressed", "true", { timeout: 2_000 });
  await split.click();
  await expect(unified).toHaveAttribute("aria-pressed", "false");
  await expect(split).toHaveAttribute("aria-pressed", "true");
  await expect(card.getByRole("button", { name: "Split", exact: true })).toHaveAttribute("aria-pressed", "true");

  await wrap.click();
  await expect(wrap).toHaveAttribute("aria-pressed", "true");
  await expect(card.getByRole("button", { name: "Wrap", exact: true })).toHaveAttribute("aria-pressed", "true");
  for (const box of [hunk, card]) {
    await expect(box.locator('pre[data-diff-type="split"]')).toHaveCount(1);
    await expect(numberOf(box, "change-deletion", "[data-deletions]")).toHaveText("1");
    await expect(numberOf(box, "change-addition", "[data-additions]")).toHaveText("1");
    await expect(rowOf(box, "change-deletion", "[data-deletions]")).toHaveText("<script>x</script>");
    await expect(rowOf(box, "change-addition", "[data-additions]")).toHaveText("<b>&new</b>");
  }

  await card.getByRole("button", { name: "Unified", exact: true }).click();
  for (const box of [hunk, card]) {
    await expect(box.locator('pre[data-diff-type="split"]')).toHaveCount(0);
    await expect(rowOf(box, "change-deletion")).toHaveText("<script>x</script>");
  }
  await expect(prefix.locator("[data-diff-layout]")).toHaveCount(0);
  await expect(prefix.locator("pre")).toHaveText(fencePainter.prefixCopy);
  await hunk.getByRole("button", { name: "Copy code", exact: true }).click();
  await prefix.getByRole("button", { name: "Copy code", exact: true }).click();
  expect(await tab.evaluate(() => (window as unknown as { s6cCopies: string[] }).s6cCopies))
    .toEqual([fencePainter.raw, fencePainter.prefixCopy]);
});
