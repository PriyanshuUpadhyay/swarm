import { expect, it, vi } from "vitest";
import type { App } from "./app";
import type { Row, Snapshot } from "./api";
import { AppReader } from "./app-reader";
import { AppStream } from "./app-stream";
import * as transcript from "./transcript";

const a = "/tmp/uploads/00000000-0000-0000-0000-000000000001.png";
const b = "/tmp/uploads/00000000-0000-0000-0000-000000000002.webp";
const row = (id: string, kind: Row["kind"], order: number): Row => ({ id, kind, text: a, order: [order, 0] });

// T2b uses public calls and HAND maps that disagree with the raw text.
// The namespace probe compiles on OLD; an absent export is not behavior RED.
it("S8d images follow pages, older pages, live replacements and snapshot queues", async () => {
  let items = [{ mode: "steering", id: "queue-0", detail: "Steering", text: a, images: [b] }];
  const pages = [
    { rows: [row("u1", "user", 1), row("q2", "queued", 2)], images: { u1: [b, a], q2: [b] }, page: { start_offset: 100, end_offset: 500 } },
    { rows: [row("q0", "queued", 0)], images: { q0: [b] }, page: { start_offset: 0, end_offset: 100 } },
  ];
  const app = {
    chats: new Map(), inputs: new Map(), composing: { chair: () => ({ queue: items }) },
    api: { rows: async () => pages.shift() }, reading: {}, visible: [], liveAll: false,
    pending: new Map(), frame: 0, openId: "s", notify: () => {},
  };
  const reader = new AppReader(app as unknown as App);
  const stream = new AppStream(app as unknown as App);
  const chat = reader.newChat("s");
  const imagesOf = Reflect.get(transcript, "imagesOf") as ((row: Row) => string[]) | undefined;
  const read = (rows: Row[]) => rows.map(r => ({ id: r.id, images: imagesOf?.(r) ?? null }));
  const initialQueue = read(chat.queue);
  await reader.loadTail(chat);
  const page = read(chat.rows);
  await reader.loadOlder("s");
  const older = read(chat.rows);
  vi.stubGlobal("requestAnimationFrame", () => 1);
  const send = (data: Record<string, unknown>) => {
    stream.onSse({ event: "rows", data: JSON.stringify({ key: "s:chair", end_offset: 600, ...data }) });
    stream.flush();
  };
  try {
    send({ upsert: [row("u1", "user", 1)], images: { u1: [b] } });
    const live = read(chat.rows);
    send({ upsert: [row("u1", "user", 1)] });
    const cleared = read(chat.rows);
    items = [{ mode: "steering", id: "served-1", detail: "Steering", text: a, images: [b, a] }];
    stream.applySnapshot({ version: 2, sessions: [], agents: {} } as Snapshot);
    const refreshedQueue = read(chat.queue);
    // HAND: same id, text and detail; only the served images change, so the queue must redraw.
    items = [{ mode: "steering", id: "served-1", detail: "Steering", text: a, images: [a] }];
    stream.applySnapshot({ version: 3, sessions: [], agents: {} } as Snapshot);
    const imagesOnlyQueue = read(chat.queue);
    items = [];
    stream.applySnapshot({ version: 4, sessions: [], agents: {} } as Snapshot);
    if (!imagesOf) throw new Error("S8D_ABSENT_API: transcript.imagesOf; not behavior RED");
    expect({ initialQueue, page, older, live, cleared, refreshedQueue, imagesOnlyQueue, finalQueue: read(chat.queue) }).toEqual({
      initialQueue: [{ id: "queue-0", images: [b] }],
      page: [{ id: "u1", images: [b, a] }, { id: "q2", images: [b] }],
      older: [{ id: "u1", images: [b, a] }, { id: "q0", images: [b] }, { id: "q2", images: [b] }],
      live: [{ id: "u1", images: [b] }, { id: "q0", images: [b] }, { id: "q2", images: [b] }],
      cleared: [{ id: "u1", images: [] }, { id: "q0", images: [b] }, { id: "q2", images: [b] }],
      refreshedQueue: [{ id: "served-1", images: [b, a] }],
      imagesOnlyQueue: [{ id: "served-1", images: [a] }],
      finalQueue: [],
    });
  } finally {
    vi.unstubAllGlobals();
  }
});
