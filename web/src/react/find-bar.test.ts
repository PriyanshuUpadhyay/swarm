// S10e-1 frozen RED tests: the Find bar waits for POST /api/rows. The owner ruling 2026-11-10 says it
// shows "Searching..." while the request runs and an error line when it fails, never "No matches" for
// a failure, and "Output too large to search" after a 413. Matches come from the ORIGINAL capture.
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HAND_READ, ORIGINAL_PARSER, errorReply, findHarness, matchesReply, stubBrowser, waitFor } from "../core/find-harness";
import { PaneBody } from "./transcript";

beforeEach(stubBrowser);
afterEach(() => vi.unstubAllGlobals());

/** What the owner reads in the pane: the markup with its tags removed. */
const shown = (h: ReturnType<typeof findHarness>) =>
  renderToStaticMarkup(createElement(PaneBody, { app: h.app, pane: h.app.view, empty: "" })).replace(/<[^>]*>/g, " ");

async function search(query: string) {
  const h = findHarness();
  await h.open();
  h.app.openFind();
  h.app.setFindQuery(query);
  return h;
}

describe("find waits for the server", () => {
  it("shows Searching... while the request runs, not No matches", async () => {
    const h = await search("parser");
    expect(await waitFor(() => !!h.sent("parser")), "the browser sends POST /api/rows").toBe(true);
    expect(shown(h)).toMatch(/Searching(\.\.\.|\u2026)/);
    expect(shown(h)).not.toContain("No matches");
  });

  it("drops the answer of an older query and keeps the newest", async () => {
    const h = await search("parser");
    expect(await waitFor(() => !!h.sent("parser")), "the first query is sent").toBe(true);
    h.app.setFindQuery("read");
    expect(await waitFor(() => !!h.sent("read")), "the second query is sent").toBe(true);
    h.sent("read")!.answer(matchesReply(HAND_READ));
    expect(await waitFor(() => h.app.view.find.matches.length === 1), "the newest answer is applied").toBe(true);
    h.sent("parser")!.answer(matchesReply(ORIGINAL_PARSER));
    await new Promise((r) => setTimeout(r, 50));
    expect(h.app.view.find.query).toBe("read");
    expect(h.app.view.find.matches.map((m) => m.id)).toEqual(["read-config"]);
    const card = (id: string) => h.app.view.lines.find((l) => l.id === id)?.open;
    expect([card("read-config"), card("parser-check")]).toEqual([true, false]);
  });

  it("shows an error line when the request fails, not No matches", async () => {
    const h = await search("parser");
    expect(await waitFor(() => !!h.sent("parser")), "the browser sends POST /api/rows").toBe(true);
    h.sent("parser")!.answer(errorReply(500, "worker failed"));
    expect(await waitFor(() => !/Searching/.test(shown(h))), "the pending text goes away").toBe(true);
    expect(shown(h)).toMatch(/fail|error|unable|could not/i);
    expect(shown(h)).not.toContain("No matches");
  });

  it("shows Output too large to search after a 413", async () => {
    const h = await search("parser");
    expect(await waitFor(() => !!h.sent("parser")), "the browser sends POST /api/rows").toBe(true);
    h.sent("parser")!.answer(errorReply(413, "request body too large"));
    expect(await waitFor(() => shown(h).includes("Output too large to search")), "the 413 text is shown").toBe(true);
    expect(shown(h)).not.toContain("No matches");
  });
});
