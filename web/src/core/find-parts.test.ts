import { describe, expect, it } from "vitest";
import type { Api, Row, RowPage } from "./api";
import { App } from "./app";
import { findMatches, noteFind } from "./transcript";
import servedMap from "../../../tests/serve/find-map.json";
import handMap from "./fixtures/find-hand-map.json";
import inputs from "./fixtures/find-inputs.json";
import captured0 from "./fixtures/find-capture-0.json";
import captured1 from "./fixtures/find-capture-1.json";
import captured2 from "./fixtures/find-capture-2.json";
import captured3 from "./fixtures/find-capture-3.json";
import captured4 from "./fixtures/find-capture-4.json";
import captured5 from "./fixtures/find-capture-5.json";
import captured6 from "./fixtures/find-capture-6.json";
import captured7 from "./fixtures/find-capture-7.json";

type Part = { f: string; p?: string } | { t: string };
type FindMap = Record<string, Part[]>;
type Full = { state: string; text?: string };
type Set = { id: string; rows: Row[]; fulls: { id: string; entries: [string, Full][] | null }[] };
const expected = new Map([...captured0,...captured1,...captured2,...captured3,...captured4,...captured5,...captured6,...captured7].map((c) => [c.id, c.matches]));
const sets = inputs.row_sets as unknown as Set[];
const maps: Record<string, FindMap> = { "s7a-served": servedMap, "s7a-hand": handMap };

function check(rows: Row[], set: Set) {
  for (const variant of set.fulls) {
    const full = variant.entries === null ? undefined : new Map(variant.entries);
    for (const query of inputs.queries) {
      const id = `${set.id}/${variant.id}/${query.id}`;
      const actual = findMatches(rows, query.q, full).map((m) => ({ ...m, fold: m.fold?.id ?? null }));
      expect(actual, id).toEqual(expected.get(id));
    }
  }
}

describe("find parts from the server", () => {
  it("keeps ORIGINAL matches through direct maps, stream flush, and both page paths", async () => {
    for (const set of sets) {
      const fresh = () => structuredClone(set.rows);
      const find = maps[set.id];
      const direct = fresh();
      noteFind(direct, find);
      check(direct, set);

      const page = (older: number): RowPage => ({ rows: fresh(), find, usage: null,
        page: { start_offset: older, end_offset: 500 } });
      const api = { rows: async (_session: string, _agent: string, _tail: number, before?: number) => page(before ? 0 : 100) };
      const app = new App(api as unknown as Api);
      const chat = app.reading.newChat("s");
      await app.reading.loadTail(chat);
      check(chat.rows, set);
      // Fresh objects here ensure the older-page path must attach its own map.
      chat.rows = [];
      await app.reading.loadOlder("s");
      check(chat.rows, set);

      chat.rows = [];
      app.pending.set("s:chair", [{ key: "s:chair", end_offset: 600, upsert: fresh(), find }]);
      app.streaming.flush();
      check(chat.rows, set);
    }
    // An older server supplies no map. Its source text is not a fallback.
    expect(findMatches([{ id: "unmapped", kind: "assistant", text: "alpha", order: [1, 0] }], "alpha")).toEqual([]);
  });
});
