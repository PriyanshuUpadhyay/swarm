import { Buffer } from "node:buffer";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Api, type FindSnapshot, type Row, type RowPage } from "../core/api";
import { App } from "../core/app";
import { stubBrowser, waitFor } from "../core/find-harness";
import { PaneBody } from "./transcript";

beforeEach(stubBrowser);
afterEach(() => vi.unstubAllGlobals());

it("shows Output too large to search without sending a snapshot above 16 MiB", async () => {
  // The astral character makes UTF-8 bytes differ from JavaScript string length.
  const row: Row = { id: "p", kind: "assistant", text: "\u{1f600}", order: [1, 0] };
  const snapshot: FindSnapshot = { version: 1, query: "needle", rows: [row], find: { p: [{ f: "text" }] }, full: {} };
  const size = 16 * 1024 * 1024 + 1;
  row.text += "x".repeat(size - Buffer.byteLength(JSON.stringify(snapshot)));
  expect(Buffer.byteLength(JSON.stringify(snapshot))).toBe(size);
  const page: RowPage = { page: { start_offset: 0, end_offset: 1 }, rows: [row], usage: null, find: snapshot.find as RowPage["find"], display: { p: { blocks: [] } } };
  const fetch = vi.fn<typeof globalThis.fetch>(async (_url, init) => init?.method === "POST"
    ? Response.json({ error: "connection reset" }, { status: 500 })
    : Response.json(page));
  const app = new App(new Api("token", fetch));
  app.openChat("chat");
  expect(await waitFor(() => app.view.chatState === "ready")).toBe(true);
  fetch.mockClear();
  app.openFind();
  app.setFindQuery("needle");
  expect(await waitFor(() => app.view.find.pending === false)).toBe(true);
  const shown = renderToStaticMarkup(createElement(PaneBody, { app, pane: app.view, empty: "" })).replace(/<[^>]*>/g, " ");
  expect(shown).toContain("Output too large to search");
  expect(fetch).not.toHaveBeenCalled();
});
