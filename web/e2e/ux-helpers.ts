import { expect, type Page } from "@playwright/test";
import type { Row, RowPage } from "../src/core/api";

export const row = (id: string, text: string, kind: Row["kind"] = "assistant"): Row => ({ id, kind, text, order: [Number(id.replace(/\D/g, "")) || 1, 0] });
export const rows = (items: Row[], older = 0): RowPage => ({ rows: items, page: { start_offset: older, end_offset: 500 }, usage: null });
export async function open(tab: Page, session = process.env.SWARM_E2E_LIVE!) {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.getByTestId("transcript")).toHaveAttribute("data-state", /ready|error/);
}
export async function mockRows(tab: Page, items: Row[], older = 0) {
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: rows(items, older) }));
}
