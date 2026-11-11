import { describe, expect, it } from "vitest";
import type { Row } from "./api";
import { FoldChoices, lines } from "./transcript";

const row = (id: string, offset: number, extra: Partial<Row> = {}): Row => ({ id, kind: "assistant", text: id, order: [offset, 0], ...extra });
const unmapped = (id: string, offset: number) => row(id, offset, { kind: "system", system_kind: "unmapped", collapsed: true });
const state = { folds: new FoldChoices(), opened: new Map<string, boolean>() };

describe("unmapped records", () => {
  it("make one count line at the end, and none when there are none", () => {
    const rows = [row("a1", 1), unmapped("x1", 2), unmapped("x2", 3), row("r1", 4, { kind: "system", system_kind: "record", collapsed: true }), unmapped("x3", 5)];
    const ls = lines(rows, state);
    expect(ls.map((l) => l.id)).toEqual(["a1", "unmapped"]);
    expect(ls[1].row.text).toBe("3 items this app does not show yet");
    expect((ls[1].row.steps ?? []).map((r) => r.id)).toEqual(["x1", "x2", "x3"]);
    expect(lines([row("a1", 1)], state).length).toBe(1);
    expect(lines([unmapped("x1", 1)], state)[0].row.text).toBe("1 item this app does not show yet");
  });
});
