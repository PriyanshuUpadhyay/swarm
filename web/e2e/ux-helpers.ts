import { expect, type Page } from "@playwright/test";
import type { Display, PatchInfo, Row, RowPage } from "../src/core/api";

export const row = (id: string, text: string, kind: Row["kind"] = "assistant"): Row => ({ id, kind, text, order: [Number(id.replace(/\D/g, "")) || 1, 0] });
function displayOf(item: Row): Display {
  if (item.text.startsWith("| CLI | Name form | Note |\n")) {
    return { blocks: [{ type: "table", value: {
      source: item.text,
      align: ["none", "none", "none"],
      head: [[{ type: "text", value: "CLI" }], [{ type: "text", value: "Name form" }], [{ type: "text", value: "Note" }]],
      rows: [[[{ type: "text", value: "Claude" }], [{ type: "code", value: "swarm:plan" }], [{ type: "text", value: "plain words that wrap ".repeat(40).trimEnd() }]]],
    } }] };
  }
  return { blocks: item.text.split(/\n[ \t]*\n/).filter(Boolean).map((text) => ({ type: "paragraph", value: [{ type: "text", value: text }] })) };
}
export const rows = (items: Row[], older = 0, patch: Record<string, PatchInfo> = {}): RowPage => ({
  rows: items,
  patch,
  page: { start_offset: older, end_offset: 500 },
  usage: null,
  display: Object.fromEntries(items.map((item) => [item.id, displayOf(item)])),
});
export async function open(tab: Page, session = process.env.SWARM_E2E_LIVE!) {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.getByTestId("transcript")).toHaveAttribute("data-state", /ready|error/);
}
export async function mockRows(tab: Page, items: Row[], older = 0, patch: Record<string, PatchInfo> = {}) {
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: rows(items, older, patch) }));
}
