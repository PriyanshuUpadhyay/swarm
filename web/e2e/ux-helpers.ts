import { expect, type Locator, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import type { Block, Display, FindPart, Inline, PatchInfo, Row, RowPage } from "../src/core/api";

export const row = (id: string, text: string, kind: Row["kind"] = "assistant"): Row => ({ id, kind, text, order: [Number(id.replace(/\D/g, "")) || 1, 0] });
function displayOf(item: Row): Display {
  if (item.kind === "proposedPlan" && item.text === "## Steps\n\n1. Add the row in `src/row.rs` and wire it") {
    return { blocks: [
      { type: "heading", value: { depth: 2, children: [{ type: "text", value: "Steps" }] } },
      { type: "list", value: { ordered: true, start: 1, tight: true, items: [[
        { type: "paragraph", value: [{ type: "text", value: "Add the row in " }, { type: "code", value: "src/row.rs" }, { type: "text", value: " and wire it" }] },
      ]] } },
    ] };
  }
  if (item.text.startsWith("| CLI | Name form | Note |\n")) {
    return { blocks: [{ type: "table", value: {
      source: item.text,
      align: ["none", "none", "none"],
      head: [[{ type: "text", value: "CLI" }], [{ type: "text", value: "Name form" }], [{ type: "text", value: "Note" }]],
      rows: [[[{ type: "text", value: "Claude" }], [{ type: "code", value: "swarm:plan" }], [{ type: "text", value: "plain words that wrap ".repeat(40).trimEnd() }]]],
    } }] };
  }
  const fence = item.text.match(/^```(\w+)\n([\s\S]*)```$/);
  if (fence) return { blocks: [{ type: "code", value: { label: fence[1], text: fence[2] } }] };
  return { blocks: item.text.split(/\n[ \t]*\n/).filter(Boolean).map((text) => ({ type: "paragraph", value: [{ type: "text", value: text }] })) };
}
export function bodyMode(display: Display | null | undefined): "tree" | "plain" {
  const ready = (node: Block | Inline): boolean => {
    switch (node.type) {
      case "later": return false;
      case "paragraph": case "blockquote": case "emphasis": case "strong": case "delete":
        return node.value.every(ready);
      case "heading": case "link": return node.value.children.every(ready);
      case "list": return node.value.items.every((item) => item.every(ready));
      case "table": return [...node.value.head, ...node.value.rows.flat()].every((cell) => cell.every(ready));
      case "text": case "code": case "break": case "thematic_break": case "image": return true;
    }
  };
  return display != null && display.blocks.every(ready) ? "tree" : "plain";
}
export const rows = (items: Row[], older = 0, patch: Record<string, PatchInfo> = {}, find: Record<string, FindPart[]> = {}): RowPage => {
  const display = Object.fromEntries(items.map((item) => [item.id, displayOf(item)]));
  return {
    rows: items,
    patch,
    find,
    page: { start_offset: older, end_offset: 500 },
    usage: null,
    display,
    body: Object.fromEntries(Object.entries(display).map(([id, tree]) => [id, bodyMode(tree)])),
  };
};
export async function open(tab: Page, session = process.env.SWARM_E2E_LIVE!) {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.getByTestId("transcript")).toHaveAttribute("data-state", /ready|error/);
}
export async function mockRows(tab: Page, items: Row[], older = 0, patch: Record<string, PatchInfo> = {}, find: Record<string, FindPart[]> = {}) {
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: rows(items, older, patch, find) }));
}

export async function captureThemes(page: Page, target: Locator | Page, name: string) {
  const dir = process.env.DESIGN_B_SHOTS;
  if (!dir) return;
  mkdirSync(dir, { recursive: true });
  for (const theme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await page.evaluate((value) => { document.documentElement.dataset.theme = value; }, theme);
    await target.screenshot({ path: resolve(dir, `${name}-${theme}.png`), animations: "disabled" });
  }
}

type StreamFixture = {
  send: (event: string, data: unknown) => void;
  drop: () => void;
  restore: () => void;
};

declare global {
  interface Window {
    swarmStream: StreamFixture;
  }
}

export async function mockStream(tab: Page, stream: string, first: unknown = { stream, build: "fixture" }) {
  await tab.route("**/api/subscribe", (route) => route.fulfill({ json: { ok: true } }));

  await tab.addInitScript((first) => {
    const fetch = window.fetch.bind(window);
    let down = false;
    window.fetch = async (input, init) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (!url.endsWith("/api/stream")) return fetch(input, init);
      const headers = { "Content-Type": "text/event-stream" };
      if (down) return new Response("", { headers });

      const body = new ReadableStream<Uint8Array>({ start(controller) {
        const encoder = new TextEncoder();
        const send = (event: string, data: unknown) => controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
        window.swarmStream = {
          send,
          drop: () => { down = true; controller.close(); },
          restore: () => { down = false; },
        };
        send("hello", first);
      } });
      return new Response(body, { headers });
    };
  }, first);
}

export function streamFrame(tab: Page, event: string, data: unknown) {
  return tab.evaluate(({ event, data }) => window.swarmStream.send(event, data), { event, data });
}

export function streamControl(tab: Page, action: "drop" | "restore") {
  return tab.evaluate((mode) => window.swarmStream[mode](), action);
}
