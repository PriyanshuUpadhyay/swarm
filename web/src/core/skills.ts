// The Skills page (ADR 0111): the kit's blocks, its default skills (presets) as step graphs, and a
// Composer for a new preset. Every fact comes from the kit's own `preset.py` through `/api/kit`, and
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

export type SkillsView = "catalog" | "graph" | "composer";
/** A Composer draft. `from` names the preset it started from, or null for a new skill. `offers` is the Offers line as typed. */
export type Draft = { from: string | null; name: string; description: string; stop: "owner" | "chat"; offers: string; rows: string[][] };
/** The last check, for the exact text it checked. */
export type Check = { text: string; result: Checked } | { text: string; error: string };

export type SkillsState = {
  open: boolean;
  view: SkillsView;
  kit: Load<Kit>;
  preset: string | null;
  block: string | null;
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

export type CatalogItem = { block: Block; depth: number };
export type CatalogGroup = { title: string; items: CatalogItem[] };

const KIND_TITLES: [string, string][] = [
  ["step", "Steps"],
  ["rule", "Rules"],
  ["pack", "Packs"],
];

/** The Catalog tag of a block that no preset step loads: the plan front door, offered by a preset, or unused. */
export function blockTag(kit: Kit, block: Block): "front door" | "offered" | "unused" | null {
  if (kit.front.includes(block.id)) return "front door";
  if (block.used_by.length) return null;
  return block.offered_by.length ? "offered" : "unused";
}

/** How a step loads a block; a parent that a step did not name comes in for one of its named parts. */
export function usedByLabel(block: Block, use: UsedBy) {
  if (use.named) return "named";
  return block.parent ? `through ${block.parent}` : "for a named part";
}

/** Parents with their parts under them, then the other blocks by kind; a filter keeps a parent whose part matches. */
export function catalogGroups(kit: Kit, filter: string): CatalogGroup[] {
  const q = filter.trim().toLowerCase();
  const hit = (b: Block) => !q || [b.id, b.description, b.needs, b.gives].some((t) => t.toLowerCase().includes(q));
  const byId = new Map(kit.blocks.map((b) => [b.id, b]));
  const parents = kit.blocks.filter((b) => b.kind === "parent");
  const grouped: CatalogItem[] = parents.flatMap((p) => {
    const parts = p.parts.map((id) => byId.get(id)).filter((b): b is Block => !!b);
    if (!hit(p) && !parts.some(hit)) return [];
    return [{ block: p, depth: 0 }, ...parts.filter((b) => hit(p) || hit(b)).map((block) => ({ block, depth: 1 }))];
  });
  const groups: CatalogGroup[] = grouped.length ? [{ title: "Parents and their parts", items: grouped }] : [];
  for (const [kind, title] of KIND_TITLES) {
    const items = kit.blocks.filter((b) => b.kind === kind && !b.parent && hit(b)).map((block) => ({ block, depth: 0 }));
    if (items.length) groups.push({ title, items });
  }
  return groups;
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
  state: SkillsState = { open: false, view: "catalog", kit: { state: "idle" }, preset: null, block: null, draft: blankDraft(), undo: null, check: null };
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

  selectBlock(block: string) {
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
      this.set({ kit: { state: "ready", data }, preset: this.state.preset ?? data.presets[0]?.preset ?? null });
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
