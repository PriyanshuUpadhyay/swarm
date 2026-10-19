import { describe, expect, it } from "vitest";
import { historyStep, recallOrder, recordSent, type HistoryEntry } from "./history";

const sent = (text: string, chat: string, at: number): HistoryEntry => ({ text, session: chat, at });

describe("recordSent", () => {
  it("appends, keeps one copy of a repeat from the same chat, and caps the list", () => {
    let list = recordSent([], sent("fix it", "chat-1", 1));
    list = recordSent(list, sent("fix it", "chat-1", 2));
    list = recordSent(list, sent("fix it", "chat-2", 3));
    expect(list.map((entry) => entry.session)).toEqual(["chat-1", "chat-2"]);
    const full = [sent("first", "chat-1", 1), sent("second", "chat-1", 2)];
    expect(recordSent(full, sent("third", "chat-1", 3), 2).map((entry) => entry.text)).toEqual(["second", "third"]);
  });
});

describe("recallOrder", () => {
  it("puts this chat first, then other chats of the workspace, each text once", () => {
    const list = [sent("old here", "chat-1", 1), sent("other", "chat-2", 2), sent("new here", "chat-1", 3), sent("other", "chat-1", 4)];
    expect(recallOrder(list, "chat-1")).toEqual(["other", "new here", "old here"]);
    // A new chat has no sends, so it recalls the workspace.
    expect(recallOrder(list, "fresh")).toEqual(["other", "new here", "old here"]);
  });
});

describe("historyStep", () => {
  const entries = ["newest", "two\nlines", "oldest"];

  it("starts from an empty box or caret 0 and keeps the typed draft", () => {
    expect(historyStep(entries, null, "", 0, "up")).toEqual({ text: "newest", caret: 0, browse: { index: 0, draft: "" } });
    expect(historyStep(entries, null, "my draft", 0, "up")?.browse).toEqual({ index: 0, draft: "my draft" });
    // A first-line Up with the caret inside the text moves the caret, as the text area does.
    expect(historyStep(entries, null, "my draft", 3, "up")).toBeNull();
    expect(historyStep([], null, "", 0, "up")).toBeNull();
  });

  it("goes older from the first line and newer from the last line, then gives the draft back", () => {
    let step = historyStep(entries, { index: 0, draft: "mine" }, "newest", 0, "up")!;
    expect(step.text).toBe("two\nlines");
    // On the second line, Up moves the caret, not the history.
    expect(historyStep(entries, step.browse, "two\nlines", 6, "up")).toBeNull();
    // On the first line, Down moves the caret down.
    expect(historyStep(entries, step.browse, "two\nlines", 1, "down")).toBeNull();
    step = historyStep(entries, step.browse, "two\nlines", 8, "down")!;
    expect(step).toEqual({ text: "newest", caret: 6, browse: { index: 0, draft: "mine" } });
    expect(historyStep(entries, step.browse, "newest", 6, "down")).toEqual({ text: "mine", caret: 4, browse: null });
  });

  it("stops at the oldest entry, and an edited entry ends browsing", () => {
    expect(historyStep(entries, { index: 2, draft: "" }, "oldest", 0, "up")).toBeNull();
    expect(historyStep(entries, { index: 0, draft: "" }, "newest!", 7, "down")).toBeNull();
    expect(historyStep(entries, { index: 0, draft: "" }, "newest!", 0, "up")?.browse).toEqual({ index: 0, draft: "newest!" });
  });
});
