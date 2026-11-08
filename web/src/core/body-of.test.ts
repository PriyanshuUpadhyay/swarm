// Step 5(g) slice S4 (contract PAINTER-V3 R1, V2 items 4 and 5): the map that came with the rows decides how a Markdown row is painted.
// noteDisplay keeps one Display per row object, mergeRows keeps or replaces row objects, and bodyOf says what to paint:
// html (no map: today's renderer), plain (no entry, the text as it is), or tree (the Display, when it holds no later block or inline).
import { describe, expect, it } from "vitest";
import type { Display, Row } from "./api";
import { bodyOf, mergeRows, noteDisplay } from "./transcript";

// A row as the server sends it; `order` is [source offset, event ordinal].
const row = (id: string, offset: number, extra: Partial<Row> = {}): Row => ({ id, kind: "assistant", text: id, order: [offset, 0], ...extra });
const say = (text: string): Display => ({ blocks: [{ type: "paragraph", value: [{ type: "text", value: text }] }] });
// A later inline inside a quote: the check has to look through nesting.
const laterInline: Display = {
  blocks: [{ type: "blockquote", value: [{ type: "paragraph", value: [{ type: "text", value: "a" }, { type: "later", value: "inline" }] }] }],
};
const laterBlock: Display = { blocks: [{ type: "later", value: "block" }] };

describe("bodyOf", () => {
  it("follows the map of a page, of a frame that replaces and removes, and of a page without a map", () => {
    // The tail page: five rows and a fold with one step. The fold and "no-entry" have no entry in the map.
    const tail = [
      row("replaced", 10),
      row("removed", 20),
      row("untouched", 30),
      row("no-entry", 40),
      row("later-inline", 50),
      row("later-block", 55),
      row("fold", 70, { kind: "fold", steps: [row("fold-step", 60)] }),
    ];
    noteDisplay(tail, {
      replaced: say("first"),
      removed: say("gone"),
      untouched: say("kept"),
      "later-inline": laterInline,
      "later-block": laterBlock,
      "fold-step": say("step"),
    });
    let rows = mergeRows([], tail);

    // A frame: "replaced" comes again with new text and a new Display, "removed" goes, "untouched" is not in the frame.
    const upsert = [row("replaced", 10, { text: "second" })];
    noteDisplay(upsert, { replaced: say("second") });
    rows = mergeRows(rows, upsert, ["removed"]);

    // An older page with no map: its row has no note, and a call with no map changes no note.
    const older = [row("older", 5)];
    noteDisplay(older, undefined);
    rows = mergeRows(rows, older);
    noteDisplay(rows, undefined);

    const by = (id: string) => rows.find((r) => r.id === id) as Row;
    const observed = {
      ids: rows.map((r) => r.id),
      bodies: [
        ["replaced", bodyOf(by("replaced"))],
        ["untouched", bodyOf(by("untouched"))],
        ["no-entry", bodyOf(by("no-entry"))],
        ["later-inline", bodyOf(by("later-inline"))],
        ["later-block", bodyOf(by("later-block"))],
        ["fold-step", bodyOf((by("fold").steps as Row[])[0])],
        ["older", bodyOf(by("older"))],
      ],
    };

    expect(observed).toEqual({
      ids: ["older", "replaced", "untouched", "no-entry", "later-inline", "later-block", "fold"],
      bodies: [
        ["replaced", { kind: "tree", display: say("second") }],
        ["untouched", { kind: "tree", display: say("kept") }],
        ["no-entry", { kind: "plain" }],
        ["later-inline", { kind: "plain" }],
        ["later-block", { kind: "plain" }],
        ["fold-step", { kind: "tree", display: say("step") }],
        ["older", { kind: "plain" }],
      ],
    });
  });
});
