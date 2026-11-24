import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./api";
import { harness, settle } from "./composer-harness";
import { noteMessage, STORE_KEY, type Note } from "./notes";

afterEach(() => vi.unstubAllGlobals());
const store = (value: string) => {
  const saved = new Map([[STORE_KEY, value]]);
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => saved.get(key) ?? null,
    setItem: (key: string, v: string) => saved.set(key, v),
    removeItem: (key: string) => saved.delete(key),
  });
};
const note = (n: Partial<Note> & Pick<Note, "kind" | "quote" | "text">): Note => ({ id: 1, rowId: "row-1", at: 1, ...n });

describe("noteMessage", () => {
  it("numbers each note under its full quote and leaves out row ids", () => {
    const text = noteMessage([
      note({ kind: "text", rowId: "row-7", quote: "first quote line\nsecond quote line", text: "Why 10 minutes?" }),
      note({ kind: "reply", rowId: "row-9", quote: "reply line 1\n\nreply line 3", text: "Zażółć gęślą jaźń 👍\nsecond line" }),
      note({ kind: "item", quote: "C · Margin notes", text: "" }),
    ]);

    expect(text).toBe(
      [
        "Notes on your reply (3):",
        "",

        "1. > first quote line",
        "   > second quote line",
        "   Why 10 minutes?",
        "",

        "2. > [whole reply]",
        "   > reply line 1",
        "   >",
        "   > reply line 3",
        "   Zażółć gęślą jaźń 👍",
        "   second line",
        "",

        "3. > C · Margin notes",
      ].join("\n"),
    );
    expect(text).not.toContain("row-");
  });
});

describe("Notes.send", () => {
  it("keeps the drafts after a failed send and clears them once, when Retry goes", async () => {
    store("");
    let refuse = true;
    const { app, posts, snapshot } = harness(() => (refuse ? "send failed" : null));
    snapshot();
    app.openChat("chat");
    await settle();
    const sent: string[] = [];
    app.notice = (text) => sent.push(text);
    app.notes.add("chat", { kind: "item", rowId: "a1", quote: "C · Margin notes" });

    expect(await app.notes.send("chat")).toBe(false);
    expect(app.notes.list("chat")).toHaveLength(1);
    await settle();
    expect(posts).toHaveLength(1);

    refuse = false;
    app.view.composer!.state.banner!.retry();
    await settle();
    expect(posts).toHaveLength(2);
    expect(posts[1].body.text).toBe("Notes on your reply (1):\n\n1. > C · Margin notes");
    expect(app.notes.list("chat")).toEqual([]);
    expect(sent).toEqual(["Sent 1 note to the agent."]);
  });

  it("keeps the sent notes as sent while a failed send can still be retried, and frees them on Dismiss", async () => {
    store("");
    const { app, snapshot } = harness(() => "send failed");
    snapshot();
    app.openChat("chat");
    await settle();
    const added = app.notes.add("chat", { kind: "item", rowId: "a1", quote: "Q" })!;
    app.notes.setText("chat", added.id, "first");
    const sent = app.notes.list("chat");

    await app.notes.send("chat");
    app.notes.setText("chat", added.id, "edited");
    app.notes.remove("chat", added.id);
    app.view.composer!.state.banner!.retry();
    await settle();
    app.notes.setText("chat", added.id, "edited");
    expect(app.notes.list("chat")).toBe(sent);
    // A note added after the send is not in its text, so it stays open.
    const later = app.notes.add("chat", { kind: "item", rowId: "a1", quote: "R" })!;
    app.notes.setText("chat", later.id, "open");
    expect(app.notes.list("chat")[1].text).toBe("open");

    app.dismissBanner("chat");
    app.notes.setText("chat", added.id, "edited");
    expect(app.notes.list("chat")[0].text).toBe("edited");
    // After Dismiss, a typed send of the chat holds no note.
    const typed = app.send("chat", "typed");
    app.notes.setText("chat", added.id, "while typed");
    await typed;
    expect(app.notes.list("chat")[0].text).toBe("while typed");
  });

  it("keeps the notes held while Retry runs: a Send N notes meanwhile is refused", async () => {
    store("");
    let fail: (() => void) | null = null;
    const { app, posts, snapshot } = harness(
      () => (posts.length === 1 ? "send failed" : null),
      () => new Promise((_, reject) => (fail = () => reject(new ApiError(409, "send failed")))),
    );
    snapshot();
    app.openChat("chat");
    await settle();
    const added = app.notes.add("chat", { kind: "item", rowId: "a1", quote: "Q" })!;
    app.notes.setText("chat", added.id, "first");
    await app.notes.send("chat");

    app.view.composer!.state.banner!.retry();
    expect(await app.notes.send("chat")).toBe(false);
    fail!();
    await settle();

    expect(posts).toHaveLength(2);
    app.notes.setText("chat", added.id, "edited");
    expect(app.notes.list("chat")[0].text).toBe("first");
  });
});

describe("saved notes", () => {
  it("opens a chat with no drafts when the saved value has another shape", () => {
    const noQuote = { id: 1, kind: "item", rowId: "a1", text: "", at: 1 };
    // A valid note under another version, so only the version check refuses it.
    const otherVersion = JSON.stringify({ v: 2, chats: [["chat", [note({ kind: "item", quote: "Q", text: "" })]]] });
    for (const bad of ["not json", otherVersion, JSON.stringify({ v: 1, chats: [["chat", [noQuote]]] }), '{"v":1,"chats":{"chat":[]}}']) {
      store(bad);
      expect(harness().app.notes.list("chat"), bad).toEqual([]);
    }
  });

  it("keeps another tab's drafts of other chats when it saves", () => {
    store("");
    const tabA = harness().app;
    const tabB = harness().app;
    tabA.notes.add("chat-x", { kind: "text", rowId: "a1", quote: "from A" });
    tabB.notes.add("chat-y", { kind: "text", rowId: "a1", quote: "from B" });

    const later = harness().app.notes;
    expect([later.list("chat-x").length, later.list("chat-y").length]).toEqual([1, 1]);
  });

  it("says once when the browser refuses to save the notes", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: () => {
        throw new DOMException("full", "QuotaExceededError");
      },
      removeItem: () => {},
    });
    const { app } = harness();
    const said: string[] = [];
    app.notice = (text) => said.push(text);

    app.notes.add("chat", { kind: "text", rowId: "a1", quote: "one" });
    app.notes.add("chat", { kind: "text", rowId: "a1", quote: "two" });
    expect(said).toHaveLength(1);
  });

  it("refuses the 201st note of a chat with a notice", () => {
    store("");
    const { app } = harness();
    const said: string[] = [];
    app.notice = (text) => said.push(text);
    for (let i = 0; i < 200; i++) expect(app.notes.add("chat", { kind: "text", rowId: "a1", quote: `quote ${i}` })).not.toBeNull();
    expect(app.notes.add("chat", { kind: "text", rowId: "a1", quote: "one more" })).toBeNull();
    expect(app.notes.list("chat")).toHaveLength(200);
    expect(said).toHaveLength(1);
  });
});
