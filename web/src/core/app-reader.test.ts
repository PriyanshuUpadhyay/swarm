import { describe, expect, it } from "vitest";
import type { Row } from "./api";
import { liveRow } from "./app-reader";

const row = (kind: Row["kind"], text: string): Row => ({ id: text, kind, text, order: [0, 0] });

describe("liveRow", () => {
  it("shows the screen text until a log row of the same turn holds it", () => {
    const rows = [row("user", "write about bicycles"), row("toolUse", "ls docs")];
    const screen = "In 1817, German inventor Karl von Drais built a two-wheeled\nmachine that riders moved";
    expect(liveRow(rows, screen)?.text).toBe(screen);
    // The log keeps the markdown and one line per paragraph; the screen wraps and drops the marks.
    const landed = [...rows, row("assistant", "In **1817**, German inventor Karl von Drais built a two-wheeled machine that riders moved with their feet.")];
    expect(liveRow(landed, screen)).toBeNull();
    // A row of an earlier turn does not hide this turn's text.
    expect(liveRow([...landed, row("user", "again")], screen)?.id).toBe("live");
    expect(liveRow(rows, null)).toBeNull();
  });
});
