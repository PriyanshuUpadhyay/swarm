// Step 5(g) slice S5b (contract LIVE-WEB-V3, R2, R3 and V2 items 1 to 3, V3 item 1): the live row and the queue
// rows are painted from their display. Every expected tree is written by hand, so these cases compile and fail
// by assertion before the code exists. The setup is the one of app-stream.test.ts.
import { describe, expect, it, vi } from "vitest";
import frozenQueue from "../../../tests/fixtures/s8c/queue.json";
import type { Display, Snapshot } from "./api";
import type { App } from "./app";
import { AppReader } from "./app-reader";
import { AppStream } from "./app-stream";
import type { Chat } from "./app-types";
import { FoldChoices, bodyOf } from "./transcript";
import { paintedBody } from "./body-paint.fixture";

const text = (value: string) => ({ type: "text" as const, value });
const strong = (value: string) => ({ type: "strong" as const, value: [text(value)] });
const paragraph = (...value: ReturnType<typeof text | typeof strong>[]): Display => ({ blocks: [{ type: "paragraph", value }] });

// "**ab** [c](https://example.com/a/long/path)": 27 letters, and 4 units ("ab c")
const source = "**ab** [c](https://example.com/a/long/path)";
const link = { type: "link" as const, value: { url: "https://example.com/a/long/path", title: null, children: [text("c")] } };
const full: Display = { blocks: [{ type: "paragraph", value: [strong("ab"), text(" "), link] }] };
// The first typing step shows ceil(4 / 8) = 1 unit, so only the "a" of the strong.
const firstStep: Display = paragraph(strong("a"));
// "**ab** c": 3 letters, so a null frame does not keep it
const short: Display = paragraph(strong("ab"), text(" c"));
// "**b**"
const bold: Display = paragraph(strong("b"));

const frame = (data: Record<string, unknown>) => ({ event: "live", data: JSON.stringify({ key: "s:chair", thinking: null, ended: false, hide_text: false, hide_thinking: false, ...data }) });
/** The field the code adds to Chat. Read through a cast, so this file compiles before it exists. */
const shownDisplay = (chat: Chat) => (chat as unknown as { liveDisplay?: Display | null }).liveDisplay ?? null;

/** An App stand-in with one ready chair chat and no live text. */
function liveApp() {
  const chat = {
    id: "s", key: "s:chair", sessionId: "s", state: "ready", rows: [], end: 0, version: 0, shift: false,
    live: null, liveShown: 0, liveThinking: null, liveEnded: false, liveDisplay: null, hideText: false, hideThinking: false,
    queue: [], folds: new FoldChoices(), opened: new Map(),
  } as unknown as Chat;
  const app = { chats: new Map([["s", chat]]), pending: new Map(), openId: "s", notify: () => {}, reading: { changed: (c: Chat) => c.version++ } };
  const stream = new AppStream(app as unknown as App);
  const reader = new AppReader(app as unknown as App);
  /** The live row as the list builds it, or null while no live row shows. */
  const liveLine = () => reader.build(chat).find((line) => line.id === "live")?.row ?? null;
  return { stream, chat, liveLine };
}

describe("live display", () => {
  it("paints the typed-out part of the tree on the live row, and the whole text stays on the row", () => {
    vi.useFakeTimers();
    try {
      const { stream, liveLine } = liveApp();
      stream.onSse(frame({ text: source, display: full }));
      vi.advanceTimersByTime(40);
      const step = liveLine();
      const afterStep = step && { body: bodyOf(step), text: step.text };
      vi.advanceTimersByTime(1000);
      const end = liveLine();
      const atEnd = end && { body: bodyOf(end), text: end.text };
      expect({ afterStep, atEnd }).toEqual({
        afterStep: { body: { kind: "tree", display: firstStep }, text: source },
        atEnd: { body: { kind: "tree", display: full }, text: source },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps the live text and its tree after the turn ends, until the log's rows land", () => {
    vi.useFakeTimers();
    try {
      const { stream, chat, liveLine } = liveApp();
      stream.onSse(frame({ text: source, display: full }));
      vi.advanceTimersByTime(1000);
      stream.onSse(frame({ text: source, display: full, ended: true }));
      const row = liveLine();
      expect({ live: chat.live, display: shownDisplay(chat), body: row && bodyOf(row) }).toEqual({
        live: source,
        display: full,
        body: { kind: "tree", display: full },
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears the live text and its tree on a null frame that nothing keeps", () => {
    vi.useFakeTimers();
    try {
      const { stream, chat } = liveApp();
      stream.onSse(frame({ text: "**ab** c", display: short }));
      const before = { live: chat.live, display: shownDisplay(chat) };
      stream.onSse(frame({ text: null, display: null }));
      expect({ before, after: { live: chat.live, display: shownDisplay(chat) } }).toEqual({
        before: { live: "**ab** c", display: short },
        after: { live: null, display: null },
      });
    } finally {
      vi.useRealTimers();
    }
  });
});

// Live tests above keep bodyOf and the exact prefix behavior.
describe("queue display", () => {
  const queue = [
    { images: [], mode: "steering", text: "**b**", display: bold, body: "tree", id: "queue-0", detail: "Steering" },
    { images: [], mode: "steering", text: "plain note", display: null, body: "tree", id: "queue-1", detail: "Steering" },
    { images: [], mode: "steering", text: "from an older server", display: bold, id: "queue-2", detail: "Steering" },
  ];
  const bodies = (chat: Chat) => chat.queue.map(paintedBody);
  const expected = [{ kind: "tree", display: bold }, { kind: "plain" }, { kind: "plain" }];

  it("paints a queue row only with served tree and its Display when the chat is built", () => {
    vi.useFakeTimers();
    try {
      const app = { chats: new Map(), composing: { chair: () => ({ queue }) } };
      const chat = new AppReader(app as unknown as App).newChat("s");
      expect({ bodies: bodies(chat), text: chat.queue.map(r => r.text) }).toEqual({ bodies: expected, text: queue.map(q => q.text) });
    } finally { vi.useRealTimers(); }
  });

  it("paints a queue row only with served tree and its Display when a snapshot brings the item", () => {
    vi.useFakeTimers();
    try {
      let items: typeof queue = [];
      const app = { chats: new Map(), composing: { chair: () => ({ queue: items }) }, inputs: new Map(), reading: {}, visible: [], liveAll: false };
      const chat = new AppReader(app as unknown as App).newChat("s");
      items = queue;
      new AppStream(app as unknown as App).applySnapshot({ version: 2, sessions: [], agents: {} } as Snapshot);
      expect({ bodies: bodies(chat), text: chat.queue.map(r => r.text) }).toEqual({ bodies: expected, text: queue.map(q => q.text) });
    } finally { vi.useRealTimers(); }
  });
});

// S10e-2: reuse the S8c queue flow; the served mode can change by itself.
describe("S8c queue wire", () => {
  it("uses served queue choices for first paint, refresh, mode-only refresh and clear", () => {
    vi.useFakeTimers();
    try {
      let items: { mode: string; text: string; id: string; detail: string; images: string[]; display?: Display | null; body?: string | null }[] = [
        { images: [], mode: "steering", text: "**b**", id: "served-7", detail: "From wire", display: bold, body: "tree" },
        { images: [], mode: "follow-up", text: "plain note", id: "served-9", detail: "Kept label", display: null, body: "tree" },
        { images: [], mode: "steering", text: "without display", id: "served-11", detail: "Third label", display: bold, body: "future" },
      ];
      const app = { chats: new Map(), composing: { chair: () => ({ queue: items }) }, inputs: new Map(), reading: {}, visible: [], liveAll: false };
      const reader = new AppReader(app as unknown as App);
      const chat = reader.newChat("s");
      const first = { rows: chat.queue, bodies: chat.queue.map(paintedBody) };
      const stream = new AppStream(app as unknown as App);
      items = frozenQueue.wire.map(q => ({ ...q, detail: `Served ${q.detail}`, body: "tree" })) as typeof items;
      stream.applySnapshot({ version: 2, sessions: [], agents: {} } as Snapshot);
      const refreshed = { rows: chat.queue, bodies: chat.queue.map(paintedBody) };
      // These snapshots keep ids, text, labels and images; only body changes.
      const modeOnly = ["plain", null, undefined, "future"].map((body, i) => {
        items = items.map(q => ({ ...q, body }));
        stream.applySnapshot({ version: 3 + i, sessions: [], agents: {} } as Snapshot);
        return { rows: chat.queue, bodies: chat.queue.map(paintedBody) };
      });
      items = [];
      stream.applySnapshot({ version: 7, sessions: [], agents: {} } as Snapshot);
      const refreshedRows = frozenQueue.rows.map(r => ({ ...r, detail: `Served ${r.detail}` }));
      expect({ first, refreshed, modeOnly, cleared: chat.queue }).toEqual({
        first: {
          rows: [
            { id: "served-7", kind: "queued", text: "**b**", detail: "From wire", order: [9007199254740991, 0] },
            { id: "served-9", kind: "queued", text: "plain note", detail: "Kept label", order: [9007199254740991, 1] },
            { id: "served-11", kind: "queued", text: "without display", detail: "Third label", order: [9007199254740991, 2] },
          ],
          bodies: [{ kind: "tree", display: bold }, { kind: "plain" }, { kind: "plain" }],
        },
        refreshed: { rows: refreshedRows, bodies: frozenQueue.wire.map(q => ({ kind: "tree", display: q.display })) },
        modeOnly: ["plain", null, undefined, "future"].map(() => ({ rows: refreshedRows, bodies: frozenQueue.rows.map(() => ({ kind: "plain" })) })),
        cleared: [],
      });
    } finally { vi.useRealTimers(); }
  });
});
