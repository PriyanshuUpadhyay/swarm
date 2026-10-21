// A .node.test.ts runs the kit script, so tsconfig.node.json checks it with Node's types.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { App } from "./app";
import { blankDraft, catalogGroups, usedByLabel, composable, copyable, draftText, presetDraft, Skills, type Checked, type Kit } from "./skills";

const script = fileURLToPath(new URL("../../../skills/kit/skills/plan/scripts/preset.py", import.meta.url));
const python = (args: string[], input = "") => JSON.parse(execFileSync("python3", ["-I", "-B", script, ...args], { encoding: "utf8", input }));
const kit: Kit = python(["--json"]);
const check = (text: string): Checked => python(["--check"], text);

describe("draftText", () => {
  it("copies each composable preset's table so that Python passes it with the same steps", () => {
    expect(composable(kit).length).toBeGreaterThan(0);
    for (const preset of composable(kit)) {
      const out = check(draftText(presetDraft(preset)));
      expect(out.ok, preset.preset).toBe(true);
      if (out.ok) expect(out.preset.steps.map((s) => [s.file, s.stage, s.resolved])).toEqual(preset.steps.map((s) => [s.file, s.stage, s.resolved]));
    }
  });

  it("gives a new skill a whole SKILL.md with its name, use line, and section", () => {
    const draft = { ...blankDraft(), name: "mine", description: "Use when testing.", stop: "chat" as const };
    draft.rows = [
      ["01-read", "none", "the sources", "local-research, requirements", "the answer rests on them"],
      ["02-reply", "read", "the answer", "voice-reply", "the owner reads it"],
    ];
    const text = draftText(draft);
    expect(text.startsWith('---\nname: mine\ndescription: "Use when testing."\n---\n\n# mine\n\n## Preset\n\nStop: chat\n')).toBe(true);
    expect(check(text).ok).toBe(true);
    expect(check(draftText({ ...draft, description: "Use when: the owner asks" })).ok).toBe(true);
    expect(check(draftText({ ...draft, name: "My skill" }))).toEqual({ ok: false, defect: "name needs a lowercase letter first, then letters, digits, and dashes" });
  });

  it("leaves a pipe in a cell as typed, so Python's defect names the row", () => {
    const draft = { ...presetDraft(composable(kit)[0]) };
    draft.rows = draft.rows.map((r, i) => (i === 0 ? [r[0], r[1], "a | b", r[3], r[4]] : r));
    const out = check(draftText(draft));
    expect(out.ok).toBe(false);
    if (!out.ok) expect(out.defect).toMatch(/^row has 6 cells, not 5: /);
  });
});

describe("catalogGroups", () => {
  it("lists every block once, with each part under its parent", () => {
    const items = catalogGroups(kit, "").flatMap((g) => g.items);
    expect(items.map((i) => i.block.id).sort()).toEqual(kit.blocks.map((b) => b.id).sort());
    const voice = items.findIndex((i) => i.block.id === "voice");
    expect(items[voice + 1]).toMatchObject({ depth: 1, block: { parent: "voice" } });
  });

  it("keeps a parent when only one of its parts matches the filter", () => {
    const items = catalogGroups(kit, "voice-reply").flatMap((g) => g.items.map((i) => i.block.id));
    expect(items).toEqual(["voice", "voice-reply"]);
  });
});

/** A Skills model whose api answers from `answer` and records each call. */
function model(answer: (path: string, body?: Record<string, unknown>) => Promise<unknown>) {
  const calls: string[] = [];
  const app = {
    api: {
      get: (path: string) => (calls.push(`GET ${path}`), answer(path)),
      post: (path: string, body: Record<string, unknown>) => (calls.push(`POST ${path}`), answer(path, body)),
    },
  };
  return { skills: new Skills(app as unknown as App), calls };
}

describe("usedByLabel", () => {
  it("says a parent comes in for a named part, and a part through its parent", () => {
    const blocks = new Map(kit.blocks.map((b) => [b.id, b]));
    const parent = blocks.get("voice")!;
    const part = blocks.get("voice-reply")!;
    expect(parent.used_by.filter((u) => !u.named).map((u) => usedByLabel(parent, u))).toContain("for a named part");
    expect(usedByLabel(part, { preset: "x", step: "y", named: false })).toBe("through voice");
    expect(usedByLabel(part, { preset: "x", step: "y", named: true })).toBe("named");
  });
});

describe("Skills", () => {
  it("turns Copy on only for the exact text that Python passed", async () => {
    const { skills } = model(async (path, body) => (path === "/api/kit" ? kit : check(String(body!.text))));
    await skills.load();
    const draft = presetDraft(composable(kit)[0]);
    skills.setDraft(draft);
    expect(copyable(skills.state)).toBe(false);
    await skills.check();
    expect(copyable(skills.state)).toBe(true);
    skills.setDraft({ ...draft, stop: draft.stop === "owner" ? "chat" : "owner" });
    expect(copyable(skills.state)).toBe(false);
    skills.setDraft({ ...draft, rows: [...draft.rows, ["99-extra", "nothing", "x", "requirements", "y"]] });
    await skills.check();
    expect(skills.state.check).toMatchObject({ result: { ok: false } });
    expect(copyable(skills.state)).toBe(false);
  });

  it("keeps the newer check when an older reply arrives last", async () => {
    let release: (v: Checked) => void = () => {};
    const slow = new Promise<Checked>((r) => (release = r));
    let first = true;
    const { skills } = model(async (path) => {
      if (path === "/api/kit") return kit;
      if (first) return (first = false), slow;
      return { ok: false, defect: "newer" };
    });
    const older = skills.check();
    await skills.check();
    release({ ok: true, preset: kit.presets[0] });
    await older;
    expect(skills.state.check).toMatchObject({ result: { ok: false, defect: "newer" } });
  });

  it("keeps the newer kit read when an older failure arrives last", async () => {
    let fail: (e: Error) => void = () => {};
    let first = true;
    const { skills } = model(async () => {
      if (first) return (first = false), new Promise((_, reject) => (fail = reject));
      return kit;
    });
    const older = skills.load();
    await skills.load();
    fail(new Error("old"));
    await older;
    expect(skills.state.kit.state).toBe("ready");
  });

  it("shows the server's error when the kit cannot be read", async () => {
    const { skills, calls } = model(async () => {
      throw new Error("swarm kit: python3 not found; the Skills page needs Python 3.11 or newer");
    });
    skills.show();
    await skills.load();
    expect(skills.state.open).toBe(true);
    expect(skills.state.kit).toEqual({ state: "error", error: "swarm kit: python3 not found; the Skills page needs Python 3.11 or newer" });
    expect(calls.filter((c) => c === "GET /api/kit").length).toBe(2);
  });
});
