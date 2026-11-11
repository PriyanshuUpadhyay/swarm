// Step 5(g) slice S5b (contract LIVE-WEB-V3, R1 "drops every container left empty"): a cut drops only the
// containers that the cut empties. An empty table cell before the cut stays, so the later cells keep their
// columns. Review of web-candidate-01, fix 1. The tree and the expected value are written by hand.
import { describe, expect, it } from "vitest";
import type { Display } from "./api";
import { cut } from "./transcript";

const text = (value: string) => ({ type: "text" as const, value });

// "| a | | c |\n|---|---|---|\n| 1 | | 3 |\n\npq": units "ac13pq"
const source = "| a | | c |\n|---|---|---|\n| 1 | | 3 |";
const table = {
  type: "table" as const,
  value: { source, align: ["none" as const, "center" as const, "right" as const], head: [[text("a")], [], [text("c")]], rows: [[[text("1")], [], [text("3")]]] },
};
const tree: Display = { blocks: [table, { type: "paragraph", value: [text("pq")] }] };

describe("live cut with empty cells", () => {
  it("keeps an empty cell before the cut so the columns stay", () => {
    expect(cut(tree, 5)).toEqual({ blocks: [table, { type: "paragraph", value: [text("p")] }] });
  });
});
