// Step 5(g) slice S5b (contract LIVE-WEB-V3, R1 and V2 items 4 and 5): `units` gives the visible text of a
// Display in reading order, and `cut` keeps the first n units of it. The trees and the expected values are
// written by hand from the contract, not taken from a candidate.
import { describe, expect, it } from "vitest";
import type { Display } from "./api";
import { cut, units } from "./transcript";

const text = (value: string) => ({ type: "text" as const, value });
const paragraph = (...value: ReturnType<typeof text>[]) => ({ type: "paragraph" as const, value });

// R1 example: "**Note** see `x`"
const note: Display = {
  blocks: [{ type: "paragraph", value: [{ type: "strong", value: [text("Note")] }, text(" see "), { type: "code", value: "x" }] }],
};

// "- one\n- two"
const list: Display = {
  blocks: [{ type: "list", value: { ordered: false, start: null, tight: true, items: [[paragraph(text("one"))], [paragraph(text("two"))]] } }],
};

// A table with a head row and two body rows
const source = "| ab | cd |\n|---|---|\n| ef | gh |\n| ij | kl |";
const table: Display = {
  blocks: [
    {
      type: "table",
      value: {
        source,
        align: ["none", "right"],
        head: [[text("ab")], [text("cd")]],
        rows: [
          [[text("ef")], [text("gh")]],
          [[text("ij")], [text("kl")]],
        ],
      },
    },
  ],
};

// Every kind of node that adds text, in one tree: a heading, a quote, a break, a link, a fence
const mixed: Display = {
  blocks: [
    { type: "heading", value: { depth: 2, children: [text("T")] } },
    { type: "blockquote", value: [paragraph(text("q"))] },
    { type: "paragraph", value: [text("a"), { type: "break" }, { type: "link", value: { url: "https://e.test/", title: null, children: [text("b")] } }] },
    { type: "code", value: { label: null, text: "c" } },
  ],
};

// A thematic break and `later` nodes add no unit
const silent: Display = {
  blocks: [paragraph(text("a")), { type: "thematic_break" }, { type: "later", value: "html" }, { type: "paragraph", value: [text("b"), { type: "later", value: "html" }] }],
};

describe("live cut", () => {
  it("counts the visible units of a tree and keeps the first n of them", () => {
    const actual = {
      units: [note, list, table, mixed, silent].map(units),
      // R1 example: 6 units end inside the text " see "
      noteCut6: cut(note, 6),
      // inside item 2, and exactly at the end of item 1 (item 2 is dropped, not kept empty)
      listCut4: cut(list, 4),
      listCut3: cut(list, 3),
      // inside the first body row: the head stays, the later cell and the second row go
      tableCut7: cut(table, 7),
      // the silent nodes drop with the later text
      silentCut1: cut(silent, 1),
      // n at or past the total gives the same object
      same: [cut(note, 10) === note, cut(note, 99) === note, cut(table, 12) === table],
    };
    const expected = {
      units: ["Note see x", "onetwo", "abcdefghijkl", "Tqa\nbc", "ab"],
      noteCut6: { blocks: [{ type: "paragraph", value: [{ type: "strong", value: [text("Note")] }, text(" s")] }] },
      listCut4: {
        blocks: [{ type: "list", value: { ordered: false, start: null, tight: true, items: [[paragraph(text("one"))], [paragraph(text("t"))]] } }],
      },
      listCut3: { blocks: [{ type: "list", value: { ordered: false, start: null, tight: true, items: [[paragraph(text("one"))]] } }] },
      tableCut7: {
        blocks: [{ type: "table", value: { source, align: ["none", "right"], head: [[text("ab")], [text("cd")]], rows: [[[text("ef")], [text("g")]]] } }],
      },
      silentCut1: { blocks: [paragraph(text("a"))] },
      same: [true, true, true],
    };
    expect(actual).toEqual(expected);
  });
});
