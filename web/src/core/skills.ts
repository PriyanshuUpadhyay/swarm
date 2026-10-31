// The Skills page (ADR 0111): the kit's blocks as the run tree of a starting plan beside the block
// library, and a Composer for a new preset. Every fact comes from the kit's own `preset.py` through `/api/kit`, and
// every draft check through `/api/kit/check`; this module parses no Needs text and checks nothing.

import type { App } from "./app";
import { message } from "./app-types";
import type { Load } from "./changes";
import type { Command } from "./commands";

export type UsedBy = { preset: string; step: string; named: boolean };
export type Block = { id: string; kind: string; description: string; needs: string; gives: string; parent: string | null; parts: string[]; used_by: UsedBy[]; offered_by: string[]; duty: boolean };
/** `cells` is the table row as written; `blocks` are the ids it names, `resolved` what it loads. */
export type Step = { file: string; needs: string; holds: string; why: string; cells: string[]; blocks: string[]; resolved: string[]; uses: string[]; stage: number };
export type Preset = { preset: string; verb: string; mode: string | null; stop: "owner" | "chat"; start: string | null; offers: string[]; steps: Step[] };
/** `front` is the plan front door, which runs before step 1 of every preset. */
export type Kit = { schema: string; version: string; front: string[]; blocks: Block[]; presets: Preset[] };
export type Checked = { ok: true; preset: Preset } | { ok: false; defect: string };

export type SkillsView = "catalog" | "composer";
/** A Composer draft. `from` names the preset it started from, or null for a new skill. `offers` is the Offers line as typed. */
export type Draft = { from: string | null; name: string; description: string; stop: "owner" | "chat"; offers: string; rows: string[][] };
/** The last check, for the exact text it checked. */
export type Check = { text: string; result: Checked } | { text: string; error: string };

export type SkillsState = {
  open: boolean;
  view: SkillsView;
  kit: Load<Kit>;
  /** The "Start from" plan of the Blocks page: a preset name, or BLANK. */
  preset: string;
  /** The selected block of the Blocks page, or null. */
  block: string | null;
  filter: string;
  draft: Draft;
  /** The edited draft that the last new start replaced, until the next edit. */
  undo: Draft | null;
  check: Check | null;
};

const HEADER = "| File | Needs | Holds | Block | Why |\n|---|---|---|---|---|";
const CHECK_DELAY_MS = 300;

export function blankDraft(): Draft {
  return { from: null, name: "my-skill", description: "Use when …", stop: "owner", offers: "", rows: [["01-frame", "none", "", "requirements", ""]] };
}

/** A draft from a preset copies its rows as written, so its Copy is the preset's own table text. */
export function presetDraft(preset: Preset): Draft {
  return { from: preset.preset, name: preset.verb, description: "", stop: preset.stop, offers: preset.offers.join(", "), rows: preset.steps.map((s) => [...s.cells]) };
}

/** The text that Copy gives: the `## Preset` section, inside a whole SKILL.md for a new skill. */
export function draftText(draft: Draft): string {
  const rows = draft.rows.map((cells) => `| ${cells.map((c) => c.trim()).join(" | ")} |`).join("\n");
  const offers = draft.offers.trim() ? `Offers: ${draft.offers.trim()}\n` : "";
  const section = `## Preset\n\nStop: ${draft.stop}\n${offers}\n${HEADER}\n${rows}\n`;
  if (draft.from) return section;
  // A JSON string is a YAML double-quoted string, so a `: ` in the use line cannot break the front matter.
  return `---\nname: ${draft.name.trim()}\ndescription: ${JSON.stringify(draft.description.trim())}\n---\n\n# ${draft.name.trim()}\n\n${section}`;
}

/** Copy is on only when Python passed exactly the text that Copy would give. */
export function copyable(state: Pick<SkillsState, "draft" | "check">): boolean {
  const check = state.check;
  return !!check && "result" in check && check.result.ok && check.text === draftText(state.draft);
}

/** Presets the Composer may start from: one with a `Start:` line runs its own steps, so it is view only. */
export function composable(kit: Kit): Preset[] {
  return kit.presets.filter((p) => p.start === null);
}

/** The "Start from" choice with no preset: only the front door, and the planner adds every step. */
export const BLANK = "blank";

export function planOf(kit: Kit, preset: string): Preset | null {
  return kit.presets.find((p) => p.preset === preset) ?? null;
}

/** The step files of the plan that name a block, in step order. */
export function stepsLoading(kit: Kit, preset: string, id: string): string[] {
  return (planOf(kit, preset)?.steps ?? []).filter((s) => s.blocks.includes(id)).map((s) => s.file);
}

export function offeredIn(kit: Kit, preset: string): Block[] {
  return planOf(kit, preset) ? kit.blocks.filter((b) => b.offered_by.includes(preset)) : [];
}

/** A step's child: one block, or one family node for the parts of one parent that the step names. */
export type StepChild = { block: string } | { family: string; parts: string[] };

/**
 * A step's blocks, with the named parts of one family under one family node. A step loads only the parts
 * it names, so the node lists those parts and never the others. Front-door blocks stay single.
 */
export function stepChildren(kit: Kit, step: Step): StepChild[] {
  const byId = new Map(kit.blocks.map((b) => [b.id, b]));
  const out: StepChild[] = [];
  const families = new Map<string, { family: string; parts: string[] }>();
  for (const id of step.blocks) {
    const parent = kit.front.includes(id) ? null : (byId.get(id)?.parent ?? null);
    if (!parent) {
      out.push({ block: id });
      continue;
    }
    let node = families.get(parent);
    if (!node) {
      node = { family: parent, parts: [] };
      families.set(parent, node);
      out.push(node);
    }
    node.parts.push(id);
  }
  return out;
}

export function planSummary(kit: Kit, preset: string): string {
  const plan = planOf(kit, preset);
  if (!plan) return "Blank plan: only the front door. The planner adds every step and block for the task.";
  const named = new Set(plan.steps.flatMap((s) => s.blocks));
  const offered = offeredIn(kit, preset).length;
  const rest = kit.blocks.filter((b) => !named.has(b.id) && !b.offered_by.includes(preset) && !kit.front.includes(b.id)).length;
  return `${plan.steps.length} steps load ${named.size} blocks, ${offered} offered, ${rest} only in the library.`;
}

/** Where a block sits in the chosen plan, as one sentence. */
export function whereInPlan(kit: Kit, preset: string, block: Block): string {
  if (kit.front.includes(block.id)) return "Front door. It runs first for every task.";
  const here = stepsLoading(kit, preset, block.id);
  if (here.length) return `In this plan at ${here.join(", ")}.`;
  if (block.offered_by.includes(preset)) return "In the offered tray. Off until the planner adds it.";
  if (block.kind === "parent") {
    const parts = block.parts.filter((p) => stepsLoading(kit, preset, p).length);
    if (parts.length) return `Not named in this plan. Its parts ${parts.join(", ")} load on their own.`;
  }
  return "Not in this plan. The planner can add it.";
}

/** The presets whose steps name a block (or, for a parent, load it for a part), with those steps. */
export function loadedBy(block: Block): [string, string[]][] {
  const out = new Map<string, string[]>();
  for (const u of block.used_by) if (u.named || block.kind === "parent") out.set(u.preset, [...(out.get(u.preset) ?? []), u.step]);
  return [...out];
}

/** The library: blocks by id with each family's parts under it; a query matches id, description, needs, or gives. */
export function libraryRows(kit: Kit, query: string): { block: Block; part: boolean }[] {
  const q = query.trim().toLowerCase();
  const byId = new Map(kit.blocks.map((b) => [b.id, b]));
  const rows = [...kit.blocks]
    .filter((b) => !b.parent || !byId.has(b.parent))
    .sort((x, y) => x.id.localeCompare(y.id))
    .flatMap((b) => [{ block: b, part: false }, ...b.parts.flatMap((p) => (byId.has(p) ? [{ block: byId.get(p)!, part: true }] : []))]);
  if (!q) return rows;
  return rows.filter(({ block: b }) => [b.id, b.description, b.needs, b.gives].some((t) => t.toLowerCase().includes(q))).map((r) => ({ ...r, part: false }));
}

/** The library row's tag: front, the step numbers that load it, or offered. */
export function libraryTag(kit: Kit, preset: string, id: string): string {
  if (kit.front.includes(id)) return "front";
  const steps = stepsLoading(kit, preset, id);
  if (steps.length) return steps.map((f) => f.split("-")[0]).join(" ");
  return kit.blocks.find((b) => b.id === id)?.offered_by.includes(preset) ? "offered" : "";
}

/** The first sentence of a description, for a one-line row. */
export function firstSentence(text: string): string {
  return text.split(/(?<=\.)\s/)[0];
}

/** A kit older than the app has no front door, duty, or offers; they are empty then, so the page still draws. */
function withDefaults(kit: Kit): Kit {
  return {
    ...kit,
    front: kit.front ?? [],
    blocks: kit.blocks.map((b) => ({ ...b, offered_by: b.offered_by ?? [], duty: b.duty ?? false })),
    presets: kit.presets.map((p) => ({ ...p, offers: p.offers ?? [] })),
  };
}

export class Skills {
  state: SkillsState = { open: false, view: "catalog", kit: { state: "idle" }, preset: "implement", block: null, filter: "", draft: blankDraft(), undo: null, check: null };
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** Count reads and checks, so an older reply cannot replace a newer one. */
  private loads = 0;
  private checks = 0;

  constructor(private readonly app: App) {}

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private set(change: Partial<SkillsState>) {
    this.state = { ...this.state, ...change };
    for (const fn of this.listeners) fn();
  }

  /** Opens the page. The kit is read once per open, so an upgraded kit shows on the next open. */
  show() {
    this.set({ open: true });
    // load() keeps its own failure in state.kit, so nothing waits for it.
    void this.load();
  }

  close() {
    this.set({ open: false });
  }

  setView(view: SkillsView) {
    this.set({ view });
  }

  setFilter(filter: string) {
    this.set({ filter });
  }

  selectBlock(block: string | null) {
    this.set({ block });
  }

  selectPreset(preset: string) {
    this.set({ preset });
  }

  async load() {
    this.set({ kit: { state: "loading" } });
    const seq = ++this.loads;
    try {
      const data = withDefaults(await this.app.api.get<Kit>("/api/kit"));
      if (seq !== this.loads) return;
      // A plan that this kit no longer has falls back to its first preset.
      const known = this.state.preset === BLANK || data.presets.some((p) => p.preset === this.state.preset);
      this.set({ kit: { state: "ready", data }, preset: known ? this.state.preset : (data.presets[0]?.preset ?? BLANK) });
      this.scheduleCheck(0);
    } catch (e) {
      if (seq === this.loads) this.set({ kit: { state: "error", error: message(e) } });
    }
  }

  /** A new draft keeps its text on screen at once; Python checks it after a short pause. */
  setDraft(draft: Draft, undo: Draft | null = null) {
    this.set({ draft, undo });
    this.scheduleCheck(CHECK_DELAY_MS);
  }

  private scheduleCheck(delay: number) {
    if (this.timer) clearTimeout(this.timer);
    // check() keeps its own failure in state.check, so nothing waits for it.
    this.timer = setTimeout(() => void this.check(), delay);
  }

  async check() {
    const text = draftText(this.state.draft);
    const seq = ++this.checks;
    try {
      const result = await this.app.api.post<Checked>("/api/kit/check", { text });
      if (seq === this.checks) this.set({ check: { text, result } });
    } catch (e) {
      if (seq === this.checks) this.set({ check: { text, error: message(e) } });
    }
  }

  commands(): Command[] {
    return [{ id: "skills", title: "Skills…", run: () => this.show() }];
  }
}
