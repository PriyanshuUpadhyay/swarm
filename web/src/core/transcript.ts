// Rows built in Rust (`/api/rows`, `rows` frames) -> the lines the list draws. The client only
// merges rows by id, places them by `order`, and keeps the reader's open and closed choices.

import type { Row } from "./api";

/** A queued row waits for the agent, so it sits after every logged row. */
function compare(a: Row, b: Row): number {
  const qa = a.kind === "queued" ? 1 : 0;
  const qb = b.kind === "queued" ? 1 : 0;
  return qa - qb || a.order[0] - b.order[0] || a.order[1] - b.order[1];
}

/** Applies `remove` and `upsert` by id. A known row keeps its place; a new row goes in by `order`.
 *  Returns the same array when nothing changed. */
export function mergeRows(rows: readonly Row[], upsert: readonly Row[] = [], remove: readonly string[] = []): Row[] {
  if (!upsert.length && !remove.length) return rows as Row[];
  const dead = new Set(remove);
  const fresh = new Map(upsert.map((r) => [r.id, r]));
  const kept: Row[] = [];
  for (const row of rows) {
    if (dead.has(row.id) && !fresh.has(row.id)) continue;
    const next = fresh.get(row.id);
    fresh.delete(row.id);
    kept.push(next ?? row);
  }
  if (!fresh.size) return kept;
  const added = [...fresh.values()].sort(compare);
  // Live rows land after the last one, so the common case is an append.
  if (!kept.length || compare(added[0], kept[kept.length - 1]) >= 0) return kept.concat(added);
  const out: Row[] = [];
  let i = 0;
  for (const row of kept) {
    while (i < added.length && compare(added[i], row) < 0) out.push(added[i++]);
    out.push(row);
  }
  while (i < added.length) out.push(added[i++]);
  return out;
}

/** The reader's open or closed choice for each fold (ADR 0047). It is stored under every step id,
 *  so it survives a run that gains steps and a fold whose own id changes; the newest choice wins. */
export class FoldChoices {
  private map = new Map<string, { open: boolean; at: number }>();
  private clock = 0;

  set(fold: Row, open: boolean) {
    const at = ++this.clock;
    for (const step of fold.steps ?? []) this.map.set(step.id, { open, at });
  }

  isOpen(fold: Row): boolean {
    let best: { open: boolean; at: number } | undefined;
    for (const step of fold.steps ?? []) {
      const c = this.map.get(step.id);
      if (c && (!best || c.at > best.at)) best = c;
    }
    return best ? best.open : !!fold.default_open;
  }
}

/** One list line: a row, a fold, or a step of an open fold. `open` is the fold's or the card's state. */
export type Line = { id: string; row: Row; fold: Row | null; open: boolean };

export type LineState = { showHidden: boolean; folds: FoldChoices; opened: ReadonlyMap<string, boolean> };

/** A failed tool card starts open; the reader's choice wins. */
export const cardOpen = (row: Row, opened: ReadonlyMap<string, boolean>) => opened.get(row.id) ?? row.tool?.state === "failed";

export function lines(rows: readonly Row[], s: LineState): { lines: Line[]; hidden: number } {
  const out: Line[] = [];
  let hidden = 0;
  for (const row of rows) {
    if (row.hidden) {
      hidden++;
      if (!s.showHidden) continue;
    }
    if (row.kind !== "fold") {
      out.push({ id: row.id, row, fold: null, open: cardOpen(row, s.opened) });
      continue;
    }
    const open = s.folds.isOpen(row);
    out.push({ id: row.id, row, fold: null, open });
    for (const step of row.steps ?? []) {
      if (step.hidden) hidden++;
      if (open && (s.showHidden || !step.hidden)) out.push({ id: step.id, row: step, fold: row, open: cardOpen(step, s.opened) });
    }
  }
  return { lines: out, hidden };
}

const searchCache = new WeakMap<Row, string>();

/** Everything a reader can see in a row, opened, in lower case. */
export function searchText(row: Row): string {
  let t = searchCache.get(row);
  if (t !== undefined) return t;
  if (row.system_kind === "swarm_ring") t = "new swarm message";
  else {
    const parts: (string | undefined)[] = [row.kind === "fold" ? row.fold?.text : row.text, row.detail, row.output?.text, row.diff?.patch];
    const tool = row.tool;
    if (tool) parts.push(tool.name, tool.header_title, tool.command, tool.path, tool.output?.text, tool.skill_body?.text, ...(tool.diffs ?? []).map((d) => d.patch));
    if (row.shell) parts.push(row.shell.command, row.shell.output.text);
    if (row.command) parts.push(row.command.name, row.command.arguments, row.command.skill_body?.text, row.command.output?.text);
    t = parts.filter(Boolean).join("\n").toLowerCase();
  }
  searchCache.set(row, t);
  return t;
}

/** A find match: the row, and the fold that holds it when it is a step. */
export type Match = { id: string; fold: Row | null };

/** Matches over every loaded row, in list order, including the steps of closed folds. */
export function findMatches(rows: readonly Row[], query: string, showHidden: boolean): Match[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: Match[] = [];
  for (const row of rows) {
    if (row.hidden && !showHidden) continue;
    if (searchText(row).includes(q)) out.push({ id: row.id, fold: null });
    for (const step of row.steps ?? []) if ((showHidden || !step.hidden) && searchText(step).includes(q)) out.push({ id: step.id, fold: row });
  }
  return out;
}

/** "0.4s", "8.6s", "2m 5s". */
export function duration(seconds: number): string {
  if (seconds < 60) return `${seconds < 10 ? seconds.toFixed(1) : Math.round(seconds)}s`;
  const m = Math.floor(seconds / 60);
  return `${m}m ${Math.round(seconds - m * 60)}s`;
}

export type PatchLine = { kind: "add" | "del" | "ctx" | "hunk" | "meta"; text: string; old?: number; new?: number };

/** Parses a unified patch into numbered lines. Header lines before the first hunk are "meta". */
export function parsePatch(patch: string): PatchLine[] {
  const out: PatchLine[] = [];
  let o = 0;
  let n = 0;
  let inHunk = false;
  for (const text of patch.split("\n")) {
    const h = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text);
    if (h) {
      o = Number(h[1]);
      n = Number(h[2]);
      inHunk = true;
      out.push({ kind: "hunk", text });
    } else if (!inHunk) {
      if (text) out.push({ kind: "meta", text });
    } else if (text.startsWith("+")) out.push({ kind: "add", text: text.slice(1), new: n++ });
    else if (text.startsWith("-")) out.push({ kind: "del", text: text.slice(1), old: o++ });
    else if (text.startsWith(" ")) out.push({ kind: "ctx", text: text.slice(1), old: o++, new: n++ });
    else if (text.startsWith("\\")) out.push({ kind: "meta", text });
  }
  return out;
}

export type SplitRow = { left: PatchLine | null; right: PatchLine | null } | { hunk: string };

/** Side by side: context on both sides; a run of removals pairs with the run of additions after it. */
export function splitPatch(lines: readonly PatchLine[]): SplitRow[] {
  const out: SplitRow[] = [];
  for (let i = 0; i < lines.length; ) {
    const l = lines[i];
    if (l.kind === "hunk") {
      out.push({ hunk: l.text });
      i++;
    } else if (l.kind === "ctx") {
      out.push({ left: l, right: l });
      i++;
    } else if (l.kind === "del" || l.kind === "add") {
      const dels: PatchLine[] = [];
      const adds: PatchLine[] = [];
      while (lines[i]?.kind === "del") dels.push(lines[i++]);
      while (lines[i]?.kind === "add") adds.push(lines[i++]);
      for (let k = 0; k < Math.max(dels.length, adds.length); k++) out.push({ left: dels[k] ?? null, right: adds[k] ?? null });
    } else i++;
  }
  return out;
}
