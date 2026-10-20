import { describe, expect, it } from "vitest";
import type { Api, Row, RowPage } from "./api";
import { App } from "./app";
import { FoldChoices, findMatches, lines } from "./transcript";

// A row as the server sends it; `order` is [source offset, event ordinal].
const row = (id: string, offset: number, extra: Partial<Row> = {}): Row => ({ id, kind: "assistant", text: id, order: [offset, 0], ...extra });
const tool = (id: string, offset: number, state: "finished" | "failed" = "finished"): Row =>
  row(id, offset, { kind: "toolUse", tool: { name: "Bash", input: {}, state, header_title: id } });
const fold = (id: string, steps: Row[], extra: Partial<Row> = {}): Row =>
  row(id, steps[0].order[0], { kind: "fold", text: `${steps.length} commands`, steps, ...extra });
const ids = (rows: { id: string }[]) => rows.map((r) => r.id);

describe("folds and lines", () => {
  const steps = [tool("read-config", 100), row("model-notice", 101, { kind: "notice", collapsed: true }), tool("run-tests", 102)];

  it("draws a closed fold as one line, and an open fold with only visible steps", () => {
    const state = { folds: new FoldChoices(), opened: new Map() };
    expect(ids(lines([fold("run", steps)], state))).toEqual(["run"]);
    expect(ids(lines([fold("run", steps, { default_open: true })], state))).toEqual(["run", "read-config", "run-tests"]);
  });

  it("skips a collapsed row even when the reader opened it", () => {
    const record = row("record", 1, { kind: "system", system_kind: "record", collapsed: true });
    expect(lines([record], { folds: new FoldChoices(), opened: new Map() })).toEqual([]);
    expect(lines([record], { folds: new FoldChoices(), opened: new Map([["record", true]]) })).toEqual([]);
  });

  it("skips collapsed folds with all their steps, and collapsed failed tools", () => {
    const hiddenFold = fold("hidden-run", steps, { collapsed: true, default_open: true });
    const hiddenTool = { ...tool("hidden-failed", 200, "failed"), collapsed: true };
    const rows = [row("before", 1), hiddenFold, hiddenTool, row("after", 300)];
    expect(ids(lines(rows, { folds: new FoldChoices(), opened: new Map() }))).toEqual(["before", "after"]);
    expect(findMatches([hiddenFold], "read-config")).toEqual([]);
  });

  it("keeps the reader's choice when the run gains a step and the fold id changes", () => {
    const folds = new FoldChoices();
    const before = fold("fold:read-config", steps.slice(0, 2), { default_open: true });
    folds.set(before, false);
    const grown = fold("fold:read-config-v2", steps, { default_open: true });
    expect(folds.isOpen(grown)).toBe(false);
    folds.set(grown, true);
    expect(folds.isOpen(fold("fold:other", [steps[2]]))).toBe(true);
    expect(folds.isOpen(fold("fold:unseen", [tool("lint", 200)]))).toBe(false);
  });

  it("opens a failed tool card by default, and the reader's choice wins", () => {
    const failed = tool("cargo-test", 5, "failed");
    expect(lines([failed], { folds: new FoldChoices(), opened: new Map() })[0].open).toBe(true);
    expect(lines([failed], { folds: new FoldChoices(), opened: new Map([["cargo-test", false]]) })[0].open).toBe(false);
  });
});

describe("find", () => {
  it("finds a step inside a closed fold and names the fold that holds it", () => {
    const rows = [row("intro", 1, { text: "Running the parser tests" }), fold("run", [tool("read-config", 10), tool("parser-check", 11)])];
    const matches = findMatches(rows, "PARSER");
    expect(matches.map((m) => [m.id, m.fold?.id ?? null])).toEqual([
      ["intro", null],
      ["parser-check", "run"],
    ]);
  });

  it("skips collapsed rows and fold steps while finding visible rows in order", () => {
    const hidden = row("model-notice", 1, { kind: "notice", text: "model: opus", collapsed: true });
    const rows = [hidden, row("opus-intro", 2), fold("run", [hidden, tool("opus-check", 3)]), row("opus-end", 4)];
    expect(findMatches(rows, "opus").map((m) => [m.id, m.fold?.id ?? null])).toEqual([
      ["opus-intro", null],
      ["opus-check", "run"],
      ["opus-end", null],
    ]);
  });

  it("opens the fold and the card of the match the reader steps to", async () => {
    const step = { ...tool("parser-check", 11), tool: { name: "Bash", input: {}, state: "finished" as const, header_title: "cargo test parser" } };
    const page: RowPage = { page: { start_offset: 0, end_offset: 20 }, rows: [fold("run", [tool("read-config", 10), step])], usage: null };
    const app = new App({ rows: async () => page } as unknown as Api);
    const g = globalThis as unknown as { requestAnimationFrame?: unknown; document?: unknown };
    g.requestAnimationFrame = () => 0;
    g.document = { querySelector: () => null };
    try {
      app.openChat("chat");
      await new Promise((r) => setTimeout(r, 0));
      expect(ids(app.view.lines)).toEqual(["run"]);
      app.openFind();
      app.setFindQuery("parser");
      expect(ids(app.view.lines)).toEqual(["run", "read-config", "parser-check"]);
      expect(app.view.lines[2].open).toBe(true);
      expect(app.view.find.matches[app.view.find.index].id).toBe("parser-check");
    } finally {
      delete g.requestAnimationFrame;
      delete g.document;
    }
  });
});

