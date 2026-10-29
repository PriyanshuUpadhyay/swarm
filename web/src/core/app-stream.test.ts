import { describe, expect, it } from "vitest";
import type { App } from "./app";
import { AppStream } from "./app-stream";
import type { Chat } from "./app-types";

/** An App stand-in whose snapshot lists `ids`, and lists `later` too after one refresh. */
function fakeApp(ids: string[], later: string[] = []) {
  const calls: string[] = [];
  let listed = ids;
  const app = {
    chatIds: () => listed,
    navigation: { refreshSnapshot: async () => (calls.push("refresh"), (listed = [...ids, ...later])) },
    openChat: (id: string) => calls.push(`open ${id}`),
  };
  return { stream: new AppStream(app as unknown as App), calls };
}

describe("show frame", () => {
  it("opens a listed chat at once", async () => {
    const { stream, calls } = fakeApp(["listed"]);
    await stream.show("listed");
    expect(calls).toEqual(["open listed"]);
  });

  it("reads a newer snapshot for a chat it does not list yet, and opens no unknown chat", async () => {
    const fresh = fakeApp([], ["newcomer"]);
    await fresh.stream.show("newcomer");
    expect(fresh.calls).toEqual(["refresh", "open newcomer"]);
    const gone = fakeApp(["listed"]);
    await gone.stream.show("stranger");
    expect(gone.calls).toEqual(["refresh"]);
  });

  it("a show frame from the stream opens the chat, and one with no id does nothing", async () => {
    const { stream, calls } = fakeApp(["listed"]);
    stream.onSse({ event: "show", data: JSON.stringify({ session: "listed" }) });
    stream.onSse({ event: "show", data: "{}" });
    await Promise.resolve();
    expect(calls).toEqual(["open listed"]);
  });
});

describe("live frame", () => {
  /** An App stand-in that holds one ready chair chat with live text on screen. */
  function liveApp() {
    const chat = { id: "s", key: "s:chair", sessionId: "s", state: "ready", rows: [], end: 0, version: 0, shift: false, live: "The history of the bicycle", liveEnded: false } as unknown as Chat;
    const app = {
      chats: new Map([["s", chat]]),
      pending: new Map(),
      openId: "s",
      notify: () => {},
      reading: { changed: (c: Chat) => c.version++ },
    };
    return { stream: new AppStream(app as unknown as App), app, chat };
  }

  it("keeps the turn's text after the turn ends, until the log's rows land", () => {
    const { stream, app, chat } = liveApp();
    stream.onSse({ event: "live", data: JSON.stringify({ key: "s:chair", text: null }) });
    expect(chat.live).toBe("The history of the bicycle");
    app.pending.set("s:chair", [{ key: "s:chair", upsert: [{ id: "a", kind: "assistant", text: "The history of the bicycle starts in 1817.", order: [1, 0] }], end_offset: 10 }]);
    stream.flush();
    expect(chat.live).toBeNull();
  });

  it("a new turn's text replaces an ended one, and a later row does not clear it", () => {
    const { stream, app, chat } = liveApp();
    stream.onSse({ event: "live", data: JSON.stringify({ key: "s:chair", text: null }) });
    stream.onSse({ event: "live", data: JSON.stringify({ key: "s:chair", text: "Next turn" }) });
    app.pending.set("s:chair", [{ key: "s:chair", upsert: [{ id: "u", kind: "user", text: "go on", order: [1, 0] }], end_offset: 10 }]);
    stream.flush();
    expect(chat.live).toBe("Next turn");
  });
});

describe("live frame at turn end", () => {
  it("drops a short spinner text, and a text the log already holds, at once", () => {
    const chat = { id: "s", key: "s:chair", sessionId: "s", state: "ready", rows: [], end: 0, version: 0, shift: false, live: "Working...", liveEnded: false } as unknown as Chat;
    const app = { chats: new Map([["s", chat]]), pending: new Map(), openId: "s", notify: () => {}, reading: { changed: (c: Chat) => c.version++ } };
    const stream = new AppStream(app as unknown as App);
    stream.onSse({ event: "live", data: JSON.stringify({ key: "s:chair", text: null }) });
    expect(chat.live).toBeNull();
    chat.rows = [{ id: "a", kind: "assistant", text: "The umbrella began more than four thousand years ago.", order: [1, 0] }];
    chat.live = "The umbrella began more than four thousand years ago.";
    stream.onSse({ event: "live", data: JSON.stringify({ key: "s:chair", text: null }) });
    expect(chat.live).toBeNull();
  });
});
