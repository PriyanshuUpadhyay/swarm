import { describe, expect, it } from "vitest";
import { harness, settle } from "./composer-harness";
import { noteMessage, type Note } from "./notes";

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
});
