// Rows built in Rust (`/api/rows`, `rows` frames) -> the lines the list draws. The client only
// skips rows the server marks `collapsed`, merges rows by id, places them by `order`, and keeps
// the reader's open and closed choices.

import type { Block, DiffPreview, Display, FindPart, Inline, PatchInfo, PatchLineWire, PatchSplitWire, Row, TextPreview } from "./api";

const patches = new WeakMap<TextPreview | DiffPreview, PatchInfo>();

/** Keep wire data on the preview object that the painter receives. */
export function notePatch(rows: Row[], patch: Record<string, PatchInfo> | undefined): void {
  if (patch === undefined) return;
  const stack = [...rows];
  while (stack.length) {
    const row = stack.pop()!;
    for (const step of row.steps ?? []) stack.push(step);
    const previews = [row.diff, ...(row.tool?.diffs ?? []), row.output, row.tool?.output, row.shell?.output, row.command?.output];
    for (const preview of previews) {
      if (!preview) continue;
      const entry = patch[preview.id];
      if (entry) patches.set(preview, entry);
      else patches.delete(preview);
    }
  }
}

export const patchOf = (preview: TextPreview | DiffPreview): PatchInfo | undefined => patches.get(preview);

/** The list keeps source indices when unified hides metadata. */
export function patchRows(info: Extract<PatchInfo, { lines: unknown }>, split: boolean): ({ i: number } | PatchSplitWire)[] {
  return split ? info.split : info.lines.flatMap((line, i) => line.kind === "meta" ? [] : [{ i }]);
}

/** Text keeps each source line; an absent entry has no colour. */
export function textRows(entry: PatchInfo | undefined, text: string): { kind: PatchLineWire["kind"] | null; text: string }[] {
  const kinds = entry && "kinds" in entry ? entry.kinds : undefined;
  return text.split("\n").map((text, i) => ({ kind: kinds?.[i] ?? null, text }));
}

const images = new WeakMap<Row, string[]>();

/** Keep only served paths on each row, including steps. A missing entry clears its previews. */
export function noteImages(rows: Row[], map: Record<string, string[]> | undefined): void {
  const stack = [...rows];
  while (stack.length) {
    const row = stack.pop()!;
    images.set(row, map?.[row.id] ?? []);
    for (const step of row.steps ?? []) stack.push(step);
  }
}

export const imagesOf = (row: Row): string[] => images.get(row) ?? [];

const displays = new WeakMap<Row, Display | null>();

export function noteDisplay(rows: Row[], display: Record<string, Display> | undefined): void {
  if (display === undefined) return;
  const stack = [...rows];
  while (stack.length) {
    const row = stack.pop()!;
    displays.set(row, display[row.id] ?? null);
    for (const step of row.steps ?? []) stack.push(step);
  }
}

export function displayOf(row: Row): Display | null | undefined {
  return displays.get(row);
}

export function bodyOf(row: Row): { kind: "plain" } | { kind: "tree"; display: Display } {
  const display = displayOf(row);
  if (display === undefined) return { kind: "plain" };
  if (display === null) return { kind: "plain" };
  const stack: (Block | Inline)[] = [...display.blocks];
  while (stack.length) {
    const node = stack.pop()!;
    switch (node.type) {
      case "later":
        return { kind: "plain" };
      case "paragraph":
      case "blockquote":
      case "emphasis":
      case "strong":
      case "delete":
        for (const child of node.value) stack.push(child);
        break;
      case "heading":
      case "link":
        for (const child of node.value.children) stack.push(child);
        break;
      case "list":
        for (const item of node.value.items) for (const child of item) stack.push(child);
        break;
      case "table":
        for (const cell of node.value.head) for (const child of cell) stack.push(child);
        for (const cells of node.value.rows) for (const cell of cells) for (const child of cell) stack.push(child);
        break;
    }
  }
  return { kind: "tree", display };
}

/** The visible UTF-16 units, in reading order, without source marks or added separators. */
export function units(d: Display): string {
  const read = (nodes: readonly (Block | Inline)[]): string => nodes.map((node): string => {
    switch (node.type) {
      case "text": return node.value;
      case "code": return typeof node.value === "string" ? node.value : node.value.text;
      case "break": return "\n";
      case "paragraph":
      case "blockquote":
      case "emphasis":
      case "strong":
      case "delete": return read(node.value);
      case "heading":
      case "link": return read(node.value.children);
      case "list": return node.value.items.map(read).join("");
      case "table": return node.value.head.map(read).join("") + node.value.rows.map((row) => row.map(read).join("")).join("");
      case "thematic_break":
      case "later": return "";
    }
  }).join("");
  return read(d.blocks);
}

/** Keeps the first n visible units; containers before the cut keep their positions. */
export function cut(d: Display, n: number): Display {
  if (n >= units(d).length) return d;
  let left = Math.max(0, n);
  const text = (value: string): string => {
    const kept = value.slice(0, left);
    left -= kept.length;
    return kept;
  };
  const takeGroups = <T extends Block | Inline>(groups: T[][]): T[][] => {
    const out: T[][] = [];
    for (const group of groups) {
      if (left <= 0) break;
      out.push(take(group));
    }
    return out;
  };
  const take = <T extends Block | Inline>(nodes: T[]): T[] => {
    const out: (Block | Inline)[] = [];
    for (const node of nodes) {
      if (left <= 0) break;
      switch (node.type) {
        case "text": {
          const value = text(node.value);
          if (value) out.push({ ...node, value });
          break;
        }
        case "code": {
          const value = text(typeof node.value === "string" ? node.value : node.value.text);
          if (value) out.push(typeof node.value === "string" ? { type: "code", value } : { type: "code", value: { ...node.value, text: value } });
          break;
        }
        case "break":
          out.push(node);
          left--;
          break;
        case "paragraph":
        case "emphasis":
        case "strong":
        case "delete": {
          const value = take(node.value);
          if (value.length) out.push({ ...node, value });
          break;
        }
        case "blockquote": {
          const value = take(node.value);
          if (value.length) out.push({ ...node, value });
          break;
        }
        case "heading": {
          const children = take(node.value.children);
          if (children.length) out.push({ ...node, value: { ...node.value, children } });
          break;
        }
        case "link": {
          const children = take(node.value.children);
          if (children.length) out.push({ ...node, value: { ...node.value, children } });
          break;
        }
        case "list": {
          const items = takeGroups(node.value.items);
          if (items.length) out.push({ ...node, value: { ...node.value, items } });
          break;
        }
        case "table": {
          const head = takeGroups(node.value.head);
          const rows: Inline[][][] = [];
          for (const row of node.value.rows) {
            if (left <= 0) break;
            rows.push(takeGroups(row));
          }
          if (head.length || rows.length) out.push({ ...node, value: { ...node.value, head, rows } });
          break;
        }
        case "thematic_break":
        case "later":
          out.push(node);
          break;
      }
    }
    // A cut changes contents, never the node kind, so the input array's type is preserved.
    return out as T[];
  };
  return { blocks: take(d.blocks) };
}

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

export type LineState = { folds: FoldChoices; opened: ReadonlyMap<string, boolean> };

/** The id of the live row of pi's streaming thinking. */
export const LIVE_THOUGHT = "live-thought";

/** A failed tool card and the streaming thinking start open; the reader's choice wins. */
export const cardOpen = (row: Row, opened: ReadonlyMap<string, boolean>) => opened.get(row.id) ?? (row.id === LIVE_THOUGHT || row.tool?.state === "failed");

/** Visible rows become lines; an open fold includes only its visible steps. */
export function lines(rows: readonly Row[], s: LineState): Line[] {
  const out: Line[] = [];
  for (const row of rows) {
    if (row.collapsed) continue;
    if (row.kind !== "fold") {
      out.push({ id: row.id, row, fold: null, open: cardOpen(row, s.opened) });
      continue;
    }
    const open = s.folds.isOpen(row);
    out.push({ id: row.id, row, fold: null, open });
    if (open) for (const step of row.steps ?? []) if (!step.collapsed) out.push({ id: step.id, row: step, fold: row, open: cardOpen(step, s.opened) });
  }
  return out;
}

const finds = new WeakMap<Row, FindPart[]>();

/** Keep the server's search parts on each row object, including steps. */
export function noteFind(rows: Row[], find: Record<string, FindPart[]> | undefined): void {
  if (find === undefined) return;
  const stack = [...rows];
  while (stack.length) {
    const row = stack.pop()!;
    finds.set(row, find[row.id] ?? []);
    for (const step of row.steps ?? []) stack.push(step);
  }
}

export const partsOf = (row: Row): readonly FindPart[] => finds.get(row) ?? [];

/** Resolve a server-chosen path within this row; only strings are searchable. */
export function partText(row: Row, part: FindPart): string {
  if ("t" in part) return part.t;
  let value: unknown = row;
  for (const key of part.f.split(".")) {
    if (value === null || typeof value !== "object" || !Object.hasOwn(value, key)) return "";
    value = (value as Record<string, unknown>)[key];
  }
  return typeof value === "string" ? value : "";
}

/** A find match: the row, and the fold that holds it when it is a step. */
export type Match = { id: string; fold: Row | null; offset: number; occurrence: number; preview?: string; line?: number; lineOccurrence?: number };

type LoadedText = ReadonlyMap<string, { state: string; text?: string }>;

/** Every occurrence in the loaded content, including text behind an expanded preview. */
export function findMatches(rows: readonly Row[], query: string, full?: LoadedText): Match[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: Match[] = [];
  const visit = (row: Row, fold: Row | null) => {
    if (row.collapsed) return;
    let occurrence = 0;
    for (const part of partsOf(row)) {
      const preview = "f" in part ? part.p : undefined;
      const loaded = preview ? full?.get(preview) : undefined;
      const value = loaded?.state === "ready" ? loaded.text : partText(row, part);
      if (!value) continue;
      const text = value.toLowerCase();
      for (let offset = text.indexOf(q); offset >= 0; offset = text.indexOf(q, offset + q.length)) {
        const before = text.slice(0, offset);
        const line = before.split("\n").length - 1;
        const onLine = before.slice(before.lastIndexOf("\n") + 1);
        out.push({ id: row.id, fold, offset, occurrence: occurrence++, ...(preview ? { preview, line, lineOccurrence: onLine.split(q).length - 1 } : {}) });
      }
    }
    for (const step of row.steps ?? []) visit(step, row);
  };
  for (const row of rows) visit(row, null);
  return out;
}

/** "0.4s", "8.6s", "2m 5s". */
export function duration(seconds: number): string {
  if (seconds < 60) return `${seconds < 10 ? seconds.toFixed(1) : Math.round(seconds)}s`;
  const m = Math.floor(seconds / 60);
  return `${m}m ${Math.round(seconds - m * 60)}s`;
}

/** Where `token` stands as its own word in `text`, or -1. */
export function wordAt(text: string, token: string): number {
  for (let i = text.indexOf(token); i >= 0; i = text.indexOf(token, i + 1)) {
    const end = i + token.length;
    if ((i === 0 || /[\s([]/.test(text[i - 1])) && (end === text.length || /\s/.test(text[end]))) return i;
  }
  return -1;
}

/** The line of the owner's prompt before or after line `from`, for ⌃⌥↑ and ⌃⌥↓; -1 when none. */
export function promptIndex(lines: readonly Line[], from: number, dir: 1 | -1): number {
  for (let i = from + dir; i >= 0 && i < lines.length; i += dir) if (lines[i].row.kind === "user" || lines[i].row.command) return i;
  return -1;
}
