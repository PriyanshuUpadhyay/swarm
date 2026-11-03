import { describe, expect, it } from "vitest";
import type { Row } from "./api";
import { liveRow, liveRows } from "./app-reader";

const row = (kind: Row["kind"], text: string): Row => ({ id: text, kind, text, order: [0, 0] });

describe("liveRow", () => {
  it("shows the screen text until a recent log row holds it", () => {
    const rows = [row("user", "write about bicycles"), row("toolUse", "ls docs")];
    const screen = "In 1817, German inventor Karl von Drais built a two-wheeled\nmachine that riders moved";
    expect(liveRow(rows, screen)?.text).toBe(screen);
    // The log keeps the markdown and one line per paragraph; the screen wraps and drops the marks.
    const landed = [...rows, row("assistant", "In **1817**, German inventor Karl von Drais built a two-wheeled machine that riders moved with their feet.")];
    expect(liveRow(landed, screen)).toBeNull();
    // pi's screen can still show the last turn's answer while the next turn starts.
    expect(liveRow([...landed, row("user", "again")], screen)).toBeNull();
    expect(liveRow(rows, null)).toBeNull();
  });

  it("never hides a short text that an earlier row happens to contain", () => {
    const rows = [row("user", "go"), row("assistant", "Everything is done except the tests.")];
    expect(liveRow(rows, "done")?.id).toBe("live");
  });
});

describe("liveRows", () => {
  it("drops pi's live thinking once the log's thought row holds it, though the turn still runs a tool", () => {
    const thinking = "I need to check that each anchor string appears as a new line in the diff.";
    expect(liveRows([row("user", "review it")], null, thinking).map((r) => r.kind)).toEqual(["thought"]);
    const landed = [row("user", "review it"), row("thought", thinking), row("toolUse", "python3 check.py")];
    expect(liveRows(landed, null, thinking)).toEqual([]);
  });
});
