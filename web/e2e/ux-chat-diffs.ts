import { expect, test } from "@playwright/test";
import { bodyMode, row, rows, open, mockRows } from "./ux-helpers";
import fencePainter from "../../tests/fixtures/s6c/painter.json" with { type: "json" };
import type { Display, PatchInfo } from "../src/core/api";
import previewPatch from "./fixtures/s6a-preview.json" with { type: "json" };
import fullLines0 from "./fixtures/s6a-full-lines-0.json" with { type: "json" };
import fullLines1 from "./fixtures/s6a-full-lines-1.json" with { type: "json" };
import fullLines2 from "./fixtures/s6a-full-lines-2.json" with { type: "json" };
import fullSplit0 from "./fixtures/s6a-full-split-0.json" with { type: "json" };

test("Find reaches repeated matches in a loaded full patch in Unified and Split views", async ({ page: tab }) => {
  const r = row("d1", "", "diff");
  r.diff = { id: "patch-output", path: "app.ts", patch: "@@ -1 +1 @@\n-old\n+preview", added: 1, removed: 1, omitted_lines: 900, shortened_lines: 0, full_lines: 1001, full_bytes: 20000 };
  await mockRows(tab, [r], 0, previewPatch as Record<string, PatchInfo>, { d1: [{ f: "diff.patch", p: "patch-output" }] });
  const patch = ["@@ -1,1000 +1,1000 @@", ...Array.from({ length: 1000 }, (_, i) => ` ${i === 100 || i === 900 ? "repeated needle" : `line ${i}`}`)].join("\n");
  await tab.route("**/api/tool-output?*", (route) => route.fulfill({ json: { text: patch, patch: { lines: fullLines0.concat(fullLines1, fullLines2), split: fullSplit0 } } }));
  await open(tab);
  await tab.getByRole("button", { name: "Show full patch", exact: true }).click();
  for (const mode of ["Unified", "Split"]) {
    await tab.getByRole("button", { name: mode, exact: true }).click();
    await tab.keyboard.press("Meta+f");
    await tab.getByTestId("find-input").fill("repeated needle");
    await expect(tab.getByTestId("find-count")).toHaveText("1 of 2");
    await expect(tab.locator('[data-output-line="901"]').first()).toBeVisible();
    await tab.getByTestId("find-input").press("Enter");
    await expect(tab.getByTestId("find-count")).toHaveText("2 of 2");
    await expect(tab.locator('[data-output-line="101"]').first()).toBeVisible();
    await tab.getByTestId("find-input").press("Escape");
  }
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
  const wrapButton = card.getByRole("button", { name: "Wrap", exact: true });
  const scrollsSideways = () => patch.evaluate((el) => el.scrollWidth > el.clientWidth);
  await expect(card.locator(".half.add .code")).toHaveText(long);
  await expect.poll(scrollsSideways).toBe(true);
  await expect.poll(() => card.locator(".half.add").evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  await tab.screenshot({ path: info.outputPath("split-scroll.png") });

  await wrapButton.click();
  await expect(wrapButton).toHaveAttribute("aria-pressed", "true");
  await expect.poll(scrollsSideways).toBe(false);
  await expect.poll(() => card.locator(".pl.split").evaluate((el) => el.getBoundingClientRect().height)).toBeGreaterThan(17 * 2);
  await expect.poll(() => patch.evaluate((el) => el.scrollHeight <= el.clientHeight)).toBe(true);
  await tab.screenshot({ path: info.outputPath("split-wrap.png") });

  await card.getByRole("button", { name: "Unified", exact: true }).click();
  await expect(wrapButton).toHaveCount(0);
  await expect.poll(scrollsSideways).toBe(true);
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
  await expect(tab.locator('[data-id="fence1"] .md-code').nth(2).locator(".pl.file")).toHaveText(["a.ts", "b.ts"]);
  for (const box of [hunk, card]) {
    await expect(box.locator(".patch")).toBeVisible();
    await expect(box.locator(".pl.del .code")).toHaveText("<script>x</script>");
    await expect(box.locator(".pl.add .code")).toHaveText("<b>&new</b>");
    await expect(box.locator(".pl.del .ln")).toHaveText(["1", ""]);
    await expect(box.locator(".pl.add .ln")).toHaveText(["", "1"]);
    await expect(box.locator("script, b")).toHaveCount(0);
  }
  await hunk.locator('[data-diff-layout="1"]').click();
  await expect(card.getByRole("button", { name: "Split", exact: true })).toHaveAttribute("aria-pressed", "true");
  await hunk.locator("[data-wrap-toggle]").click();
  await expect(card.getByRole("button", { name: "Wrap", exact: true })).toHaveAttribute("aria-pressed", "true");
  for (const box of [hunk, card]) {
    await expect(box.locator(".pl.split")).toHaveCount(1);
    await expect(box.locator(".half.del .ln")).toHaveText("1");
    await expect(box.locator(".half.add .ln")).toHaveText("1");
    await expect(box.locator(".half.del .code")).toHaveText("<script>x</script>");
    await expect(box.locator(".half.add .code")).toHaveText("<b>&new</b>");
  }
  await card.getByRole("button", { name: "Unified", exact: true }).click();
  for (const box of [hunk, card]) {
    await expect(box.locator(".pl.split")).toHaveCount(0);
    await expect(box.locator(".pl.del .code")).toHaveText("<script>x</script>");
  }
  await expect(prefix.locator("[data-diff-layout]")).toHaveCount(0);
  await expect(prefix.locator("pre")).toHaveText(fencePainter.prefixCopy);
  await hunk.getByRole("button", { name: "Copy code", exact: true }).click();
  await prefix.getByRole("button", { name: "Copy code", exact: true }).click();
  expect(await tab.evaluate(() => (window as unknown as { s6cCopies: string[] }).s6cCopies))
    .toEqual([fencePainter.raw, fencePainter.prefixCopy]);
});
