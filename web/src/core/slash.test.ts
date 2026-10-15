import { describe, expect, it } from "vitest";
import { matches, menuAt, moved, offered, pick, picksAndSends, showsEmpty, type MenuRow } from "./slash";

const row = (insert: string, extra: Partial<MenuRow> = {}): MenuRow => ({
  name: insert.slice(1),
  insert,
  kind: "command",
  description: "",
  hint: "",
  source: "",
  dialog: false,
  ...extra,
});
const rows = [
  row("/clear", { kind: "builtin", hint: "[name]" }),
  row("/model", { kind: "builtin", dialog: true }),
  row("/swarm:plan", { kind: "command" }),
  row("/plan", { kind: "builtin" }),
  row("$research", { kind: "skill" }),
];

describe("menuAt", () => {
  it("opens on a / at the start for every CLI, with the query up to the caret", () => {
    for (const provider of ["claude", "codex", "pi", "agy", null]) {
      expect(menuAt("/cle", 4, provider)).toEqual({ opener: "/", start: 0, end: 4, query: "cle" });
    }
  });

  it("opens mid-draft only after a space or bracket, and never for Codex or pi", () => {
    expect(menuAt("run /swa", 8, "claude")).toEqual({ opener: "/", start: 4, end: 8, query: "swa" });
    expect(menuAt("see (/swa", 9, "agy")?.start).toBe(5);
    expect(menuAt("run /swa", 8, "codex")).toBeNull();
    expect(menuAt("run /swa", 8, "pi")).toBeNull();
    expect(menuAt("src/main.rs", 11, "claude")).toBeNull();
  });

  it("closes once the token has a space, and opens $ only for Codex", () => {
    expect(menuAt("/clear now", 10, "claude")).toBeNull();
    expect(menuAt("use $res", 8, "codex")).toEqual({ opener: "$", start: 4, end: 8, query: "res" });
    expect(menuAt("use $res", 8, "claude")).toBeNull();
    expect(menuAt("costs 5$", 8, "codex")).toBeNull();
  });

  it("reaches to the end of the word when the caret is inside it", () => {
    expect(menuAt("/clxar more", 3, "claude")).toEqual({ opener: "/", start: 0, end: 6, query: "cl" });
  });
});

describe("offered and matches", () => {
  it("offers built-ins only at the start, and $ rows only to $", () => {
    const start = menuAt("/", 1, "claude")!;
    expect(offered(rows, start).map((r) => r.insert)).toEqual(["/clear", "/model", "/swarm:plan", "/plan"]);
    expect(offered(rows, menuAt("x /", 3, "claude")!).map((r) => r.insert)).toEqual(["/swarm:plan"]);
    expect(offered(rows, menuAt("$", 1, "codex")!).map((r) => r.insert)).toEqual(["$research"]);
    expect(showsEmpty(start)).toBe(true);
    expect(showsEmpty(menuAt("x /", 3, "claude")!)).toBe(false);
  });

  it("puts the exact name, then a prefix, before fuzzy matches", () => {
    const start = offered(rows, menuAt("/", 1, "claude")!);
    expect(matches(start, "plan").map((r) => r.insert)).toEqual(["/plan", "/swarm:plan"]);
    expect(matches(start, "swa")[0].insert).toBe("/swarm:plan");
    expect(matches(start, "").length).toBe(4);
  });
});

describe("pick", () => {
  it("replaces the whole token and adds one space", () => {
    const menu = menuAt("/cl", 3, "claude")!;
    expect(pick("/cl", menu, rows[0])).toEqual({ text: "/clear ", caret: 7 });
    const mid = menuAt("go /sw now", 6, "claude")!;
    expect(pick("go /sw now", mid, rows[2])).toEqual({ text: "go /swarm:plan now", caret: 14 });
    const inside = menuAt("/clxar", 3, "claude")!;
    expect(pick("/clxar", inside, rows[0]).text).toBe("/clear ");
  });

  it("sends a whole-draft command on the send key, hint or not, but never a dialog", () => {
    expect(picksAndSends("/cle", menuAt("/cle", 4, "claude")!, rows[0])).toBe(true);
    expect(picksAndSends("/mo", menuAt("/mo", 3, "claude")!, rows[1])).toBe(false);
    expect(picksAndSends("/cle more", menuAt("/cle more", 4, "claude")!, rows[0])).toBe(false);
    expect(picksAndSends("a /sw", menuAt("a /sw", 5, "claude")!, rows[2])).toBe(false);
    expect(picksAndSends("$res", menuAt("$res", 4, "codex")!, rows[4])).toBe(false);
  });

  it("wraps Up and Down", () => {
    expect(moved(0, 3, -1)).toBe(2);
    expect(moved(2, 3, 1)).toBe(0);
    expect(moved(0, 0, 1)).toBe(0);
  });
});
