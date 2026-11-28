// Rows built in Rust (`/api/rows`, `rows` frames) -> the lines the list draws. The client only
// skips rows the server marks `collapsed`, merges rows by id, places them by `order`, and keeps
// the reader's open and closed choices.

import { plural } from "./lifecycle-state";
import type { BodyMode, Block, DiffPreview, Display, FindPart, Inline, PatchInfo, PatchLineWire, Row, TextPreview, TodoItem } from "./api";

const patches = new WeakMap<TextPreview | DiffPreview, PatchInfo>();
/** Preview ids that the server sent with the Unified and Split layout, so their full text asks for it too. */
const layouts = new Set<string>();

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
      if (entry && "lines" in entry) layouts.add(preview.id);
    }
  }
}

export const patchOf = (preview: TextPreview | DiffPreview): PatchInfo | undefined => patches.get(preview);
export const wantsLayout = (previewId: string) => layouts.has(previewId);

/** A tool whose output the server sent as a diff layout: an agent ran `cat <file>.diff` to show a
 *  patch for review. The card shows open and outside its fold, so the owner reads it in place. */
export const showsDiff = (row: Row) => !!row.tool?.output && "lines" in (patchOf(row.tool.output) ?? {});

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

const bodies = new WeakMap<Row, BodyMode>();
/** Keep the served choice on each row object, including steps. */
export function noteBody(rows: Row[], body: Record<string, BodyMode> | null | undefined): void {
  const stack = [...rows];
  while (stack.length) {
    const row = stack.pop()!;
    bodies.set(row, body?.[row.id] ?? null);
    for (const step of row.steps ?? []) stack.push(step);
  }
}

export const bodyModeOf = (row: Row): BodyMode => bodies.get(row) ?? null;

export function paintBodyOf(row: Row): { kind: "plain" } | { kind: "tree"; display: Display } {
  const display = displayOf(row);
  return bodyModeOf(row) === "tree" && display != null ? { kind: "tree", display } : { kind: "plain" };
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
      case "image":
      case "later": return "";
    }
  }).join("");
  return read(d.blocks);
}

/** Rebuild a nested node without changing its kind. Callers own the cut budget. */
export function mapCutContainer<T extends Block | Inline>(
  node: T,
  blocks: (nodes: Block[]) => Block[],
  inlines: (nodes: Inline[]) => Inline[],
  items: (nodes: Block[][]) => Block[][],
  keepEmpty = false,
): T | null {
  switch (node.type) {
    case "paragraph": case "emphasis": case "strong": case "delete": {
      const value = inlines(node.value);
      return keepEmpty || value.length ? { ...node, value } : null;
    }

    case "blockquote": {
      const value = blocks(node.value);
      return keepEmpty || value.length ? { ...node, value } : null;
    }
    case "heading": case "link": {
      const children = inlines(node.value.children);
      return keepEmpty || children.length ? { ...node, value: { ...node.value, children } } : null;
    }

    case "list": {
      const groups = items(node.value.items);
      return keepEmpty || groups.length ? { ...node, value: { ...node.value, items: groups } } : null;
    }
    default: return node;
  }
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
        case "paragraph": case "emphasis": case "strong": case "delete":
        case "blockquote": case "heading": case "link": case "list": {
          const next = mapCutContainer(node, take, take, takeGroups);
          if (next) out.push(next);
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
        case "image":
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

export function isCliJoin(row: Row): boolean {
  return row.kind === "model_change" && row.id.startsWith(row.source_session ? `${row.source_session}:cli-join:` : "cli-join:");
}

/** A queued row waits for the agent, so it sits after every logged row. */
function compare(a: Row, b: Row): number {
  const qa = a.kind === "queued" ? 1 : 0;
  const qb = b.kind === "queued" ? 1 : 0;
  const sourceA = a.source_session ?? "~";
  const sourceB = b.source_session ?? "~";
  const segment = sourceA < sourceB ? -1 : sourceA > sourceB ? 1 : 0;
  return qa - qb || segment || Number(isCliJoin(b)) - Number(isCliJoin(a)) || a.order[0] - b.order[0] || a.order[1] - b.order[1];
}

/** A historical row ID starts with the UUID of its source session. */
export function scopedRowId(id: string): { session: string; local: string } | null {
  const match = /^([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}):(.+)$/i.exec(id);
  return match ? { session: match[1], local: match[2] } : null;
}

/** A historical output locator prefixes its log byte locator with the owning session. */
export function sourceLocator(id: string): { session: string; local: string } | null {
  const scoped = scopedRowId(id);
  return scoped && /^raw-.+$/i.test(scoped.local) ? scoped : null;
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

  /** With `full` (the owner's Full chat detail), a fold the reader has not touched starts open. */
  isOpen(fold: Row, full = false): boolean {
    let best: { open: boolean; at: number } | undefined;
    for (const step of fold.steps ?? []) {
      const c = this.map.get(step.id);
      if (c && (!best || c.at > best.at)) best = c;
    }
    return best ? best.open : full || !!fold.default_open;
  }
}

/** One list line: a row, a fold, or a step of an open fold. `open` is the fold's or the card's state.
 *  `preview` is the output that a closed tool card shows in Full chat detail. */
export type Line = { id: string; row: Row; fold: Row | null; open: boolean; preview?: string };

/** `full` is the owner's Full chat detail: folds and thinking start open, closed tool cards show a preview. */
export type LineState = { folds: FoldChoices; opened: ReadonlyMap<string, boolean>; full?: boolean };

/** The id of the live row of pi's streaming thinking. */
export const LIVE_THOUGHT = "live-thought";

/** A failed tool card and the streaming thinking start open, and in Full chat detail every thought;
 *  the reader's choice wins. */
export const cardOpen = (row: Row, opened: ReadonlyMap<string, boolean>, full = false) =>
  opened.get(row.id) ?? (row.id === LIVE_THOUGHT || (full && row.kind === "thought") || row.tool?.state === "failed" || showsDiff(row));

/** pi shows this many output lines of a closed tool call; the server cuts `tail` to the same count. */
const PREVIEW_LINES = 5;

/** The output a closed tool card shows in Full chat detail, as pi shows it: the last lines of a
 *  command, the first lines of any other tool. */
export function toolPreview(row: Row): string | undefined {
  const t = row.tool;
  if (!t?.output) return undefined;
  const lines = t.output.text.split("\n").filter((l) => l.trim());
  const text = t.command ? t.output.tail || lines.slice(-PREVIEW_LINES).join("\n") : lines.slice(0, PREVIEW_LINES).join("\n");
  return text || undefined;
}

/** A line of the list, with the output preview of a closed tool card in Full chat detail. */
function lineOf(row: Row, fold: Row | null, s: LineState): Line {
  const open = cardOpen(row, s.opened, s.full);
  const preview = s.full && !open ? toolPreview(row) : undefined;
  return preview ? { id: row.id, row, fold, open, preview } : { id: row.id, row, fold, open };
}

/** The `system_kind` of a record that no parser maps (ADR 0139). */
export const UNMAPPED = "unmapped";

/** Visible rows become lines; an open fold includes only its visible steps. Records that no parser
 *  maps stay hidden one by one, also inside a fold, and show as one count line at the end (ADR 0139). */
export function lines(rows: readonly Row[], s: LineState): Line[] {
  const out: Line[] = [];
  const unmapped: Row[] = [];

  for (const row of rows) {
    if (row.collapsed) {
      if (row.system_kind === UNMAPPED) unmapped.push(row);
      continue;
    }
    if (row.kind !== "fold") {
      out.push(lineOf(row, null, s));
      continue;
    }
    // The server nests a hidden record between two tool calls in the fold's steps.
    unmapped.push(...(row.steps ?? []).filter((step) => step.collapsed && step.system_kind === UNMAPPED));
    const open = s.folds.isOpen(row, s.full);
    out.push({ id: row.id, row, fold: null, open });
    for (const step of row.steps ?? []) if (!step.collapsed && (open || showsDiff(step))) out.push(lineOf(step, row, s));
  }

  if (unmapped.length) {
    const text = `${plural(unmapped.length, "item")} this app does not show yet`;
    const row: Row = { id: UNMAPPED, kind: "notice", system_kind: UNMAPPED, text, steps: unmapped, order: [Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER] };
    out.push({ id: row.id, row, fold: null, open: cardOpen(row, s.opened) });
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

/** The text the page draws for a step: what it is doing now while it runs, else its name. */
export const todoText = (item: TodoItem): string => (item.status === "in_progress" && item.active_text) || item.text;

/** A server match with its parent resolved against the submitted snapshot. */
export type Match = Omit<import("./api").FindMatch, "fold"> & { fold: Row | null };

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
