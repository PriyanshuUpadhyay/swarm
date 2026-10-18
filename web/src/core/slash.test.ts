import { describe, expect, it } from "vitest";
import { matches, menuAt, moved, offered, pick, picksAndSends, showsEmpty, skillFirst, toSend, type CommandList, type MenuRow } from "./slash";

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

  it("opens mid-draft only after a space or bracket, and never for Codex", () => {
    expect(menuAt("run /swa", 8, "claude")).toEqual({ opener: "/", start: 4, end: 8, query: "swa" });
    expect(menuAt("see (/swa", 9, "agy")?.start).toBe(5);
    expect(menuAt("run /swa", 8, "codex")).toBeNull();
    expect(menuAt("run /ski", 8, "pi")).toEqual({ opener: "/", start: 4, end: 8, query: "ski" });
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
    const pi = [row("/compact", { kind: "builtin" }), row("/review", { kind: "command" }), row("/skill:plan", { kind: "skill" })];
    expect(offered(pi, menuAt("/", 1, "pi")!, "pi").map((r) => r.insert)).toEqual(["/compact", "/review", "/skill:plan"]);
    expect(offered(pi, menuAt("x /", 3, "pi")!, "pi").map((r) => r.insert)).toEqual(["/skill:plan"]);
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

describe("skillFirst", () => {
  const skills = new Set(["/skill:plan", "/skill:x", "/skill:y", "/skill:my_skill"]);

  it("moves the first pi skill token to the start, where pi expands it", () => {
    expect(skillFirst("fix this /skill:plan now", skills)).toEqual({ text: "/skill:plan fix this now", extra: [] });
    expect(skillFirst("tail /skill:plan", skills)).toEqual({ text: "/skill:plan tail", extra: [] });
    expect(skillFirst("one\n/skill:plan two", skills)).toEqual({ text: "/skill:plan one\ntwo", extra: [] });
  });

  it("keeps a draft that starts with a skill, and lists a second skill that stays as text", () => {
    expect(skillFirst("/skill:plan go", skills)).toEqual({ text: "/skill:plan go", extra: [] });
    expect(skillFirst("a /skill:x b /skill:y", skills)).toEqual({ text: "/skill:x a b /skill:y", extra: ["/skill:y"] });
    expect(skillFirst("/skill:x and /skill:y", skills)).toEqual({ text: "/skill:x and /skill:y", extra: ["/skill:y"] });
  });

  it("moves only a skill that pi listed, by the name up to the next space", () => {
    expect(skillFirst("fix /skill:nosuch now", skills)).toEqual({ text: "fix /skill:nosuch now", extra: [] });
    expect(skillFirst("use /skill:my_skill here", skills)).toEqual({ text: "/skill:my_skill use here", extra: [] });
    expect(skillFirst("see /skill:x.md", skills)).toEqual({ text: "see /skill:x.md", extra: [] });
    expect(skillFirst("a/skill:x b", skills)).toEqual({ text: "a/skill:x b", extra: [] });
  });

  it("leaves a skill inside a code fence or a quote", () => {
    const fenced = "see the log\n```\n/skill:plan foo\n```";
    expect(skillFirst(fenced, skills)).toEqual({ text: fenced, extra: [] });
    const quoted = "about this:\n> + run /skill:plan now";
    expect(skillFirst(quoted, skills)).toEqual({ text: quoted, extra: [] });
    expect(skillFirst("```\nx\n```\nthen /skill:plan", skills)).toEqual({ text: "/skill:plan ```\nx\n```\nthen", extra: [] });
  });
});

describe("toSend", () => {
  const listed: CommandList = { commands: [row("/compact", { kind: "builtin" }), row("/skill:plan", { kind: "skill" })], note: null };
  const loads = (list: CommandList | null) => {
    const calls: number[] = [];
    return { calls, load: async () => (calls.push(1), list ?? Promise.reject(new Error("probe failed"))) };
  };

  it("asks pi for its skills when the menu never loaded, then moves the skill", async () => {
    const { calls, load } = loads(listed);
    expect(await toSend("fix /skill:plan now", "pi", null, load)).toEqual({ text: "/skill:plan fix now", list: listed });
    expect(calls).toHaveLength(1);
  });

  it("asks nothing when the list is there, the text has no skill, or the chat is not pi", async () => {
    const { calls, load } = loads(listed);
    expect((await toSend("fix /skill:plan now", "pi", listed, load)).text).toBe("/skill:plan fix now");
    expect(await toSend("plain words", "pi", null, load)).toEqual({ text: "plain words", list: null });
    expect(await toSend("fix /skill:plan now", "claude", null, load)).toEqual({ text: "fix /skill:plan now", list: null });
    expect(calls).toHaveLength(0);
  });

  it("sends the text as it is when the probe fails", async () => {
    const { load } = loads(null);
    expect(await toSend("fix /skill:plan now", "pi", null, load)).toEqual({ text: "fix /skill:plan now", list: null });
  });
});
