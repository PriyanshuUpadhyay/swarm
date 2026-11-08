// Step 5(g) slice S5b (contract LIVE-WEB-V3, R2, R3 and V2 items 1 to 3, V3 item 1): the live row and the queue
// rows are painted from their display. Every expected tree is written by hand, so these cases compile and fail
// by assertion before the code exists. The setup is the one of app-stream.test.ts.
import { describe, expect, it, vi } from "vitest";
import type { Display, Snapshot } from "./api";
import type { App } from "./app";
import { AppReader } from "./app-reader";
import { AppStream } from "./app-stream";
import type { Chat } from "./app-types";
import { FoldChoices, bodyOf } from "./transcript";

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

describe("queue display", () => {
  const queue = [
    { mode: "steering", text: "**b**", display: bold },
    { mode: "steering", text: "plain note", display: null },
    { mode: "steering", text: "from an older server" },
  ];
  const bodies = (chat: Chat) => chat.queue.map((row) => bodyOf(row));
  const expected = [{ kind: "tree", display: bold }, { kind: "plain" }, { kind: "plain" }];

  it("paints a queue row from its display when the chat is built", () => {
    const app = { chats: new Map(), composing: { chair: () => ({ queue }) } };
    const chat = new AppReader(app as unknown as App).newChat("s");
    expect(bodies(chat)).toEqual(expected);
  });

  it("paints a queue row from its display when a snapshot brings the item", () => {
    let items: typeof queue = [];
    const app = { chats: new Map(), composing: { chair: () => ({ queue: items }) }, inputs: new Map(), reading: {}, visible: [], liveAll: false };
    const chat = new AppReader(app as unknown as App).newChat("s");
    items = queue;
    new AppStream(app as unknown as App).applySnapshot({ version: 2, sessions: [], agents: {} } as Snapshot);
    expect(bodies(chat)).toEqual(expected);
  });
});
