import { describe, expect, it } from "vitest";
import type { Api, Row, RowPage } from "./api";
import { App } from "./app";
import { noteFind, partsOf } from "./transcript";
import servedMap from "../../../tests/serve/find-map.json";
import handMap from "./fixtures/find-hand-map.json";
import inputs from "./fixtures/find-inputs.json";

// The matching moved to POST /api/rows (tests/serve/find_rows.rs checks it against the ORIGINAL cases).
// This test keeps the transport: the browser must hold the server's find parts on every row, so it
// can send the snapshot.
type Part = { f: string; p?: string } | { t: string };
type FindMap = Record<string, Part[]>;
type Set = { id: string; rows: Row[] };
const sets = inputs.row_sets as unknown as Set[];
const maps: Record<string, FindMap> = { "s7a-served": servedMap, "s7a-hand": handMap };

function check(rows: Row[], find: FindMap, where: string) {
  const walk = (row: Row) => {
    expect(partsOf(row), `${where}: ${row.id}`).toEqual(find[row.id] ?? []);
    row.steps?.forEach(walk);
  };
  rows.forEach(walk);
}

describe("find parts from the server", () => {
  it("keeps the server's parts on the rows through direct maps, stream flush, and both page paths", async () => {
    for (const set of sets) {
      const fresh = () => structuredClone(set.rows);
      const find = maps[set.id];
      const direct = fresh();
      noteFind(direct, find);
      check(direct, find, `${set.id} direct`);

      const page = (older: number): RowPage => ({ rows: fresh(), find, usage: null,
        page: { start_offset: older, end_offset: 500 } });
      const api = { rows: async (_session: string, _agent: string, _tail: number, before?: number) => page(before ? 0 : 100) };
      const app = new App(api as unknown as Api);
      const chat = app.reading.newChat("s");
      await app.reading.loadTail(chat);
      check(chat.rows, find, `${set.id} tail`);
      // Fresh objects here ensure the older-page path must attach its own map.
      chat.rows = [];
      await app.reading.loadOlder("s");
      check(chat.rows, find, `${set.id} older`);

      chat.rows = [];
      app.pending.set("s:chair", [{ key: "s:chair", end_offset: 600, upsert: fresh(), find }]);
      app.streaming.flush();
      check(chat.rows, find, `${set.id} stream`);
    }
    // An older server supplies no map, so a row has no parts.
    expect(partsOf({ id: "unmapped", kind: "assistant", text: "alpha", order: [1, 0] })).toEqual([]);
  });
});
