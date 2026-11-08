import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Row } from "./api";
import type { App } from "./app";
import { AppReader } from "./app-reader";
import { AppStream } from "./app-stream";
import type { Chat } from "./app-types";
import { FoldChoices } from "./transcript";

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) => setTimeout(callback, 16));
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

function setup() {
  const chat = {
    id: "s", key: "s:chair", sessionId: "s", state: "ready", rows: [], end: 0, version: 0, shift: false,
    live: "Previous answer text", liveDisplay: null, liveShown: 20, liveThinking: null, liveEnded: false,
    hideText: false, hideThinking: false, queue: [], folds: new FoldChoices(), opened: new Map(),
  } as unknown as Chat;
  const app = { chats: new Map([["s", chat]]), pending: new Map(), openId: "s", notify: vi.fn(),
    reading: { changed: (c: Chat) => c.version++ } };
  const stream = new AppStream(app as unknown as App);
  return { chat, stream, reader: new AppReader(app as unknown as App) };
}
function state(chat: Chat) {
  const hides = chat as unknown as { hideText: boolean; hideThinking: boolean };
  return { text: chat.live, thinking: chat.liveThinking, display: chat.liveDisplay, ended: chat.liveEnded,
    shown: chat.liveShown, hide_text: hides.hideText, hide_thinking: hides.hideThinking };
}
const frame = (data: Record<string, unknown>) => ({ event: "live", data: JSON.stringify({ key: "s:chair",
  text: null, thinking: null, display: null, ended: false, hide_text: false, hide_thinking: false, ...data }) });
const text = "The history of the bicycle starts in Germany";

it("applies ended state and redraws a frame that changes only the hides", () => {
  const { chat, stream, reader } = setup();
  stream.onSse(frame({ text, thinking: "Check the chart", ended: true }));
  stream.flush();
  expect(state(chat)).toEqual({ text, thinking: "Check the chart", display: null, ended: true,
    shown: text.length, hide_text: false, hide_thinking: false });
  stream.onSse(frame({ text, thinking: "Check the chart" }));
  stream.flush();
  const version = chat.version;
  stream.onSse(frame({ text, thinking: "Check the chart", ended: false, hide_text: true, hide_thinking: true }));
  stream.flush();
  expect({ state: state(chat), redraw: chat.version > version,
    live: reader.build(chat).filter((line) => ["live", "live-thought"].includes(line.id)) }).toEqual({
    state: { text, thinking: "Check the chart", display: null, ended: false,
      shown: text.length, hide_text: true, hide_thinking: true }, redraw: true, live: [] });
});

it("applies the live frame after buffered rows in the same flush", () => {
  const { chat, stream } = setup();
  const row: Row = { id: "answer", kind: "assistant", text, order: [1, 0] };
  stream.onSse({ event: "rows", data: JSON.stringify({ key: "s:chair", upsert: [row], end_offset: 10 }) });
  stream.onSse(frame({ text, ended: true, hide_text: true }));
  expect({ text: chat.live, rows: chat.rows }).toEqual({ text: "Previous answer text", rows: [] });
  stream.flush();
  expect({ state: state(chat), rows: chat.rows }).toEqual({ state: {
    text, thinking: null, display: null, ended: true, shown: text.length, hide_text: true, hide_thinking: false }, rows: [row] });
});

it("shows held text when the server says show and clears it without a browser keep", () => {
  const { chat, stream, reader } = setup();
  chat.rows = [{ id: "answer", kind: "assistant", text, order: [1, 0] }];
  stream.onSse(frame({ text }));
  stream.flush();
  vi.advanceTimersByTime(1000);
  expect(reader.build(chat).filter((line) => line.id === "live").map((line) => line.row)).toEqual([
    { id: "live", kind: "assistant", text, order: [Number.MAX_SAFE_INTEGER, -1] },
  ]);
  chat.rows = [];
  stream.onSse(frame({}));
  stream.flush();
  expect(state(chat)).toEqual({ text: null, thinking: null, display: null, ended: false,
    shown: 0, hide_text: false, hide_thinking: false });
});
