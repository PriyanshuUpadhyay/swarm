import { afterEach, describe, expect, it, vi } from "vitest";
import { harness } from "./composer-harness";
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
        "",
        "   Why 10 minutes?",
        "",

        "2. > [whole reply]",
        "   > reply line 1",
        "   >",
        "   > reply line 3",
        "",
        "   Zażółć gęślą jaźń 👍",
        "   second line",
        "",

        "3. > C · Margin notes",
      ].join("\n"),
    );
    expect(text).not.toContain("row-");
  });

  it("names the child agent of a note made in its column", () => {
    const text = noteMessage([
      note({ kind: "reply", quote: "I will look at the build first.", text: "Why first?", agent: "reviewer" }),
      note({ kind: "item", quote: "Both are fixed now.", text: "" }),
    ]);

    expect(text).toBe(
      [
        "Notes on replies (2); [name] marks a child agent's reply, no name marks yours:",
        "",
        "1. [reviewer] > [whole reply]",
        "   > I will look at the build first.",
        "",
        "   Why first?",
        "",
        "2. > Both are fixed now.",
      ].join("\n"),
    );
  });
});

describe("Notes.toBox", () => {
  it("puts the notes under the box's text, removes them and ends annotate mode", () => {
    store("");
    const { app } = harness();
    app.setDraft("chat", "my own words");
    app.notes.setMode(true);
    const added = app.notes.add("chat", { kind: "item", rowId: "a1", quote: "C · Margin notes" })!;
    app.notes.setText("chat", added.id, "Pick C");

    app.notes.toBox("chat", "append");
    expect(app.draft("chat")).toBe("my own words\n\nNotes on your reply (1):\n\n1. > C · Margin notes\n\n   Pick C");
    expect(app.notes.list("chat")).toEqual([]);
    expect(app.notes.annotating()).toBe(false);
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
