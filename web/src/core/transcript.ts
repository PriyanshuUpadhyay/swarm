// Rows built in Rust (`/api/rows`, `rows` frames) -> the lines the list draws. The client only
// skips rows the server marks `collapsed`, merges rows by id, places them by `order`, and keeps
// the reader's open and closed choices.

import type { Block, Display, Inline, Row } from "./api";

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

export function bodyOf(row: Row): { kind: "html" } | { kind: "plain" } | { kind: "tree"; display: Display } {
  const display = displayOf(row);
  if (display === undefined) return { kind: "html" };
  if (display === null) return { kind: "plain" };
  const stack: (Block | Inline)[] = [...display.blocks];
  while (stack.length) {
    const node = stack.pop()!;
    switch (node.type) {
      case "later":
        return { kind: "html" };
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
export type Match = { id: string; fold: Row | null; offset: number; occurrence: number; preview?: string; line?: number; lineOccurrence?: number };

type LoadedText = ReadonlyMap<string, { state: string; text?: string }>;

/** Every occurrence in the loaded content, including text behind an expanded preview. */
export function findMatches(rows: readonly Row[], query: string, full?: LoadedText): Match[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: Match[] = [];
  const visit = (row: Row, fold: Row | null) => {
    if (row.collapsed) return;
    const parts: { text: string; preview?: string }[] = [];
    const add = (text?: string, preview?: string) => {
      const loaded = preview ? full?.get(preview) : undefined;
      const value = loaded?.state === "ready" ? loaded.text : text;
      if (value) parts.push({ text: value, preview });
    };
    const output = (p?: { id: string; text: string }) => p && add(p.text, p.id);
    const diff = (p?: { id: string; patch: string }) => p && add(p.patch, p.id);
    if (row.system_kind === "swarm_ring") add("New swarm message");
    else if (row.kind === "fold") add(row.fold?.text ?? row.text);
    else if (row.tool) {
      add(row.tool.name);
      add(row.tool.header_title ?? row.text);
      add(row.tool.command);
      add(row.tool.path);
      output(row.tool.output);
      output(row.tool.skill_body);
      row.tool.diffs?.forEach(diff);
    } else if (row.shell) {
      add(row.shell.command);
      output(row.shell.output);
    } else if (row.command) {
      add(row.command.arguments.includes(row.command.name) ? row.command.arguments : `${row.command.name} ${row.command.arguments}`);
      output(row.command.skill_body);
      output(row.command.output);
    } else if (row.kind === "toolResult") {
      if (row.output) output(row.output);
      else add(row.text);
    } else if (row.kind === "diff") diff(row.diff);
    else {
      add(row.text);
      add(row.detail);
      output(row.output);
      diff(row.diff);
    }
    let occurrence = 0;
    for (const part of parts) {
      const text = part.text.toLowerCase();
      for (let offset = text.indexOf(q); offset >= 0; offset = text.indexOf(q, offset + q.length)) {
        const before = text.slice(0, offset);
        const line = before.split("\n").length - 1;
        const onLine = before.slice(before.lastIndexOf("\n") + 1);
        out.push({ id: row.id, fold, offset, occurrence: occurrence++, ...(part.preview ? { preview: part.preview, line, lineOccurrence: onLine.split(q).length - 1 } : {}) });
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

const HUNK = /^@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@/;

/** True when the text holds a unified diff, such as `diff -u` or `git diff` output. */
export const isPatch = (text: string) => /^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@/m.test(text);

/** The colour class of each line of a text that holds unified diffs. Inside a hunk the header's
 *  counts decide where it ends, so a removed `-- comment` line is not read as a `---` header. */
export function patchKinds(lines: readonly string[]): PatchLine["kind"][] {
  let old = 0;
  let added = 0;
  return lines.map((text) => {
    if (old > 0 || added > 0) {
      if (text.startsWith("\\")) return "meta";
      const kind = text.startsWith("+") ? "add" : text.startsWith("-") ? "del" : "ctx";
      if (kind !== "add") old--;
      if (kind !== "del") added--;
      return kind;
    }
    const h = HUNK.exec(text);
    if (h) {
      old = Number(h[1] ?? 1);
      added = Number(h[2] ?? 1);
      return "hunk";
    }
    return /^(diff |index |--- |\+\+\+ |new file|deleted file|similarity|rename )/.test(text) ? "meta" : "ctx";
  });
}

/** Where `token` stands as its own word in `text`, or -1. */
export function wordAt(text: string, token: string): number {
  for (let i = text.indexOf(token); i >= 0; i = text.indexOf(token, i + 1)) {
    const end = i + token.length;
    if ((i === 0 || /[\s([]/.test(text[i - 1])) && (end === text.length || /\s/.test(text[end]))) return i;
  }
  return -1;
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

/** The line of the owner's prompt before or after line `from`, for ⌃⌥↑ and ⌃⌥↓; -1 when none. */
export function promptIndex(lines: readonly Line[], from: number, dir: 1 | -1): number {
  for (let i = from + dir; i >= 0 && i < lines.length; i += dir) if (lines[i].row.kind === "user" || lines[i].row.command) return i;
  return -1;
}
