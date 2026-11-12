import { expect, test } from "@playwright/test";
import { row, open, mockRows } from "./ux-helpers";

test("a long moved-file header keeps its controls in view", async ({ page: tab }, info) => {
  const old = "src/view/transcript/normalize/" + "previous_location/".repeat(8) + "codex_changes.rs";
  const path = "src/view/transcript/normalize/" + "next_location/".repeat(8) + "codex_changes_old.rs";
  const moved = row("move", "move", "diff");
  moved.diff = { id: "move-diff", old_path: old, path, patch: "", full_lines: 0, full_bytes: 0, omitted_lines: 0, shortened_lines: 0, added: 0, removed: 0 };
  await mockRows(tab, [moved]);
  await open(tab);

  const diff = tab.getByTestId("diff");
  await diff.evaluate((el: HTMLElement) => { el.style.width = "480px"; });
  const name = diff.locator(".diff-head > span").first();
  await expect(name).toHaveAttribute("title", `${old} -> ${path}`);
  const bounds = await diff.evaluate((el) => {
    const head = el.querySelector(".diff-head")!;
    const name = head.querySelector("span")!;
    const right = head.getBoundingClientRect().right;
    return {
      clipped: name.scrollWidth > name.clientWidth && getComputedStyle(name).textOverflow === "ellipsis",
      controlsFit: [...head.querySelectorAll("button")].every((button) => button.getBoundingClientRect().right <= right),
    };
  });
  await tab.screenshot({ path: info.outputPath("moved-file-header.png") });
  expect(bounds).toEqual({ clipped: true, controlsFit: true });
});

test("an interrupted result without its call shows its state", async ({ page: tab }, info) => {
  const result = row("partial", "partial patch output", "toolResult");
  result.tool_status = "interrupted";
  await mockRows(tab, [result]);
  await open(tab);
  await expect(tab.getByTestId("output")).toContainText("Interrupted tool result");
  await tab.screenshot({ path: info.outputPath("interrupted-tool-result.png") });
});
