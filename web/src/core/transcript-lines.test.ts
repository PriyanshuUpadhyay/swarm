import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Row } from "./api";
import { ORIGINAL_PARSER, findHarness, matchesReply, stubBrowser, waitFor } from "./find-harness";
import { FoldChoices, lines } from "./transcript";

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

  it("counts records no parser maps in one line at the end, also those inside a fold", () => {
    const unknown = (id: string, offset: number) => row(id, offset, { kind: "system", system_kind: "unmapped", collapsed: true });
    const inFold = fold("run", [tool("read-config", 100), unknown("unknown-in-fold", 101), tool("run-tests", 102)]);
    const rows = [row("reply", 1), unknown("unknown-top", 2), inFold];
    const counted = lines(rows, { folds: new FoldChoices(), opened: new Map() });
    expect(ids(counted)).toEqual(["reply", "run", "unmapped"]);
    expect(counted[2].row.text).toBe("2 items this app does not show yet");
    expect(ids(counted[2].row.steps ?? [])).toEqual(["unknown-top", "unknown-in-fold"]);
  });

  it("skips collapsed folds with all their steps, and collapsed failed tools", () => {
    const hiddenFold = fold("hidden-run", steps, { collapsed: true, default_open: true });
    const hiddenTool = { ...tool("hidden-failed", 200, "failed"), collapsed: true };
    const rows = [row("before", 1), hiddenFold, hiddenTool, row("after", 300)];
    expect(ids(lines(rows, { folds: new FoldChoices(), opened: new Map() }))).toEqual(["before", "after"]);
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

// The matcher cases that lived here (closed fold, collapsed rows, collapsed parent) are ORIGINAL cases
// lines-closed-fold, lines-collapsed and lines-collapsed-parent in tests/serve/find_rows.rs.
describe("find", () => {
  beforeEach(stubBrowser);
  afterEach(() => vi.unstubAllGlobals());

  it("opens the fold and the card of the match the reader steps to", async () => {
    const h = findHarness();
    await h.open();
    expect(ids(h.app.view.lines)).toEqual(["run"]);
    h.app.openFind();
    h.app.setFindQuery("parser");
    expect(await waitFor(() => !!h.sent("parser")), "the browser sends POST /api/rows").toBe(true);
    const request = h.sent("parser")!.body;
    expect(request.version).toBe(1);
    expect(ids(request.rows)).toEqual(["run"]);
    // Nothing opens until the server answers.
    expect(ids(h.app.view.lines)).toEqual(["run"]);
    h.sent("parser")!.answer(matchesReply(ORIGINAL_PARSER));
    expect(await waitFor(() => h.app.view.find.matches.length === 1), "the answer is applied").toBe(true);
    expect(ids(h.app.view.lines)).toEqual(["run", "read-config", "parser-check"]);
    expect(h.app.view.lines[2].open).toBe(true);
    expect(h.app.view.find.matches[h.app.view.find.index].id).toBe("parser-check");
  });
});
