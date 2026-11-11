// S10e-2 C. Frozen ORIGINAL page/frame/older outcomes plus served-choice guards.
import { describe, expect, it, vi } from "vitest";
import original from "../../../tests/fixtures/s10e2/inputs-existing.json";
import expected from "../../../tests/fixtures/s10e2/matches-existing.json";
import hand from "../../../tests/fixtures/s10e2/inputs-hand.json";
import type { Display, Row } from "./api";
import type { App } from "./app";
import { AppReader } from "./app-reader";
import { AppStream } from "./app-stream";
import { paintedBody } from "./body-paint.fixture";

const say = (text: string): Display => ({ blocks: [{ type: "paragraph", value: [{ type: "text", value: text }] }] });

describe("served saved body", () => {
  it("keeps ORIGINAL paint across a page, replacing frame and older page, and requires served tree with its Display", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", (callback: () => void) => setTimeout(callback, 16));
    try {
      const source = structuredClone(original);
      const older = [{ id: "older", kind: "assistant", text: "older", order: [5, 0] }];
      let page: unknown = {
        rows: source.tail, display: source.display,
        body: { replaced: "tree", removed: "tree", untouched: "tree", "no-entry": "tree", "later-inline": "plain", "later-block": "plain", "fold-step": "tree" },
        page: { start_offset: 5, end_offset: 100 }, usage: null,
      };
      const app = {
        chats: new Map(), pending: new Map(), openId: "s", frame: 0,
        composing: { chair: () => ({ queue: [] }) }, notify: () => {},
        api: { rows: async () => structuredClone(page) },
      };
      const reader = new AppReader(app as unknown as App);
      const stream = new AppStream(app as unknown as App);
      const chat = reader.newChat("s");
      await reader.loadTail(chat);
      const originalText = new Map(source.tail.flatMap(r => [r, ...(r.steps ?? [])]).map(r => [r.id, r.text]));
      stream.onSse({ event: "rows", data: JSON.stringify({ key: "s:chair", upsert: source.upsert, remove: source.remove, display: source.upsertDisplay, body: { replaced: "tree" }, end_offset: 120 }) });
      vi.advanceTimersByTime(16);
      // An absent body map cannot authorize this older server's raw Display.
      page = { rows: older, display: { older: say("older") }, page: { start_offset: 0, end_offset: 5 }, usage: null };
      await reader.loadOlder("s");
      const by = new Map(chat.rows.flatMap(r => [r, ...(r.steps ?? [])]).map(r => [r.id, r]));
      const observed = {
        ids: chat.rows.map(r => r.id),
        bodies: expected.bodies.map(([id]) => [id, paintedBody(by.get(String(id))!)]),
      };
      const texts = [...by.values()].map(r => [r.id, r.text]);
      const expectedTexts = [...by.values()].map(r => [r.id, r.id === "replaced" ? "second" : r.id === "older" ? "older" : originalText.get(r.id)]);

      // The mode is external input: absent, null and new values mean plain.
      const control = hand.find(c => c.id === "hand-present")!.display!.p as Display;
      const guards: Row[] = ["plain", "missing", "null", "unknown", "tree-without-display"].map((id, i) => ({ id, kind: "assistant", text: "p", order: [200 + i, 0] }));
      page = {
        rows: guards, display: Object.fromEntries(guards.filter(r => r.id !== "tree-without-display").map(r => [r.id, control])),
        body: { plain: "plain", null: null, unknown: "future", "tree-without-display": "tree" },
        page: { start_offset: 0, end_offset: 300 }, usage: null,
      };
      const guarded = reader.newChat("g");
      await reader.loadTail(guarded);
      expect({ observed, texts, guards: guarded.rows.map(r => ({ body: paintedBody(r), text: r.text })) }).toEqual({
        observed: expected, texts: expectedTexts,
        guards: guards.map(() => ({ body: { kind: "plain" }, text: "p" })),
      });
    } finally {
      vi.unstubAllGlobals();
      vi.useRealTimers();
    }
  });
});
