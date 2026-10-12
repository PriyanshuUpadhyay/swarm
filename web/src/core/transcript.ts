// Transcript events -> rows. Rows are immutable: a change replaces the row object and the array,
// so both view layers can skip unchanged rows by identity.

import type { Page, TranscriptEvent } from "./api";

export type Hunk = { old_start: number; old_lines: number; new_start: number; new_lines: number; lines: string[] };
export type Diff = { path: string; hunks: Hunk[] };

export type TextRow = { id: string; kind: "user" | "agent" | "thought" | "note"; text: string };
export type ToolRow = {
  id: string;
  kind: "tool";
  name: string;
  input: unknown;
  status: string;
  output: string | null;
  diffs: Diff[];
};
export type Row = TextRow | ToolRow;
/** One slice of an expanded tool output, rendered as its own list item. */
export type BlockItem = { id: string; kind: "block"; text: string };
export type Item = Row | BlockItem;

let seq = 0;
const nextId = () => `n:${++seq}`;

const meta = (e: TranscriptEvent) => (e.meta ?? {}) as { uuid?: string };
const str = (v: unknown) => (typeof v === "string" ? v : "");

function noteText(e: TranscriptEvent): string | null {
  switch (e.type) {
    case "system_message":
      return str(e.text);
    case "error":
      return `Error: ${str(e.message) || str(e.text) || JSON.stringify(e)}`;
    case "elicitation":
      return JSON.stringify(e.questions ?? e);
    case "image":
      return "[image]";
    case "unknown":
      return str(e.raw).slice(0, 400);
    default:
      return null;
  }
}

export class Transcript {
  rows: Row[] = [];
  page: Page = { start_offset: 0, end_offset: 0 };
  // Absolute position of each row; index = pos - first. Prepends lower `first`, so old entries stay valid.
  private pos = new Map<string, number>();
  private first = 0;

  get(id: string): Row | undefined {
    const p = this.pos.get(id);
    return p === undefined ? undefined : this.rows[p - this.first];
  }

  private set(row: Row) {
    this.rows[this.pos.get(row.id)! - this.first] = row;
  }

  private push(row: Row) {
    this.pos.set(row.id, this.first + this.rows.length);
    this.rows.push(row);
  }

  private freshId(key: string | null): string {
    return key && !this.pos.has(key) ? key : nextId();
  }

  private text(kind: TextRow["kind"], prefix: string, e: TranscriptEvent, merge: boolean) {
    const uuid = str(meta(e).uuid);
    const key = uuid ? `${prefix}:${uuid}` : null;
    const old = key && merge ? this.get(key) : undefined;
    if (old && old.kind === kind) this.set({ ...old, text: old.text + str(e.text) });
    else this.push({ id: this.freshId(key), kind, text: str(e.text) });
  }

  private tool(e: TranscriptEvent, patch: (row: ToolRow) => ToolRow) {
    const callId = str(e.tool_call_id);
    const key = callId ? `t:${callId}` : null;
    const old = key ? this.get(key) : undefined;
    if (old && old.kind === "tool") this.set(patch(old));
    else this.push(patch({ id: this.freshId(key), kind: "tool", name: "", input: null, status: "pending", output: null, diffs: [] }));
  }

  /** Appends events. Returns true when rows changed. The caller assigns `rows` a new array if needed. */
  apply(events: readonly TranscriptEvent[]): boolean {
    const before = this.rows;
    this.rows = before.slice();
    let changed = false;
    for (const e of events) {
      switch (e.type) {
        case "agent_message_chunk":
          this.text("agent", "a", e, true);
          break;
        case "agent_thought_chunk":
          this.text("thought", "th", e, true);
          break;
        case "user_message_chunk":
          this.text("user", "u", e, false);
          break;
        case "tool_call":
          this.tool(e, (r) => ({ ...r, name: str(e.name), input: e.input ?? null, status: r.output === null ? str(e.status) : r.status }));
          break;
        case "tool_call_update":
          this.tool(e, (r) => ({ ...r, status: str(e.status) || r.status, output: typeof e.content === "string" ? e.content : r.output }));
          break;
        case "tool_diff":
          this.tool(e, (r) => ({ ...r, diffs: [...r.diffs, { path: str(e.path), hunks: (e.hunks as Hunk[]) ?? [] }] }));
          break;
        default: {
          const text = noteText(e);
          if (text === null) continue;
          this.push({ id: nextId(), kind: "note", text });
        }
      }
      changed = true;
    }
    if (!changed) this.rows = before;
    return changed;
  }

  /** Adds an older page above. A row whose id is already below (a split message or a split tool call)
   *  merges into the newer row, so ids stay stable. */
  prepend(events: readonly TranscriptEvent[], page: Page) {
    const older = new Transcript();
    older.apply(events);
    const kept: Row[] = [];
    this.rows = this.rows.slice();
    for (const row of older.rows) {
      const newer = this.get(row.id);
      if (!newer) kept.push(row);
      else if (newer.kind === "tool" && row.kind === "tool")
        this.set({ ...newer, name: newer.name || row.name, input: newer.input ?? row.input, diffs: [...row.diffs, ...newer.diffs] });
      else if (newer.kind !== "tool" && row.kind !== "tool") this.set({ ...newer, text: row.text + newer.text });
      else kept.push({ ...row, id: nextId() });
    }
    this.first -= kept.length;
    kept.forEach((row, i) => this.pos.set(row.id, this.first + i));
    this.rows = [...kept, ...this.rows];
    this.page = { start_offset: page.start_offset, end_offset: this.page.end_offset };
  }
}

export function statusLabel(row: ToolRow): string {
  if (row.status === "failed") return "Failed";
  if (row.status === "completed") return "Finished";
  if (row.output === null) return row.status === "pending" || !row.status ? "Waiting for result" : row.status;
  return row.status;
}

export const PREVIEW_LINES = 120;
export const PREVIEW_CHARS = 12_000;

export type Preview = { text: string; truncated: boolean };

const previews = new WeakMap<ToolRow, Preview>();

export function previewOf(row: ToolRow): Preview {
  let p = previews.get(row);
  if (!p) previews.set(row, (p = preview(row.output ?? "")));
  return p;
}

/** The first 120 lines and 12,000 characters. It scans only the prefix, so a giant output costs little. */
export function preview(text: string): Preview {
  let end = 0;
  for (let i = 0; i < PREVIEW_LINES; i++) {
    const nl = text.indexOf("\n", end);
    if (nl < 0 || nl >= PREVIEW_CHARS) {
      end = text.length;
      break;
    }
    end = nl + 1;
  }
  end = Math.min(end, PREVIEW_CHARS);
  if (end >= text.length) return { text, truncated: false };
  return { text: text.slice(0, end), truncated: true };
}

const BLOCK_LINES = 200;
const BLOCK_CHARS = 20_000;

/** Splits a full output into list items so no single DOM node holds it all. */
export function blocks(rowId: string, text: string): BlockItem[] {
  const out: BlockItem[] = [];
  let start = 0;
  while (start < text.length) {
    let end = start;
    for (let i = 0; i < BLOCK_LINES && end < text.length; i++) {
      const nl = text.indexOf("\n", end);
      end = nl < 0 ? text.length : nl + 1;
    }
    end = Math.min(end, start + BLOCK_CHARS);
    out.push({ id: `${rowId}#${out.length}`, kind: "block", text: text.slice(start, end) });
    start = end;
  }
  return out;
}

/** Rows plus the blocks of each expanded tool output, in render order. */
export function items(rows: Row[], expanded: ReadonlySet<string>, cache: Map<string, { src: string; items: BlockItem[] }>): Item[] {
  if (expanded.size === 0) return rows;
  const out: Item[] = [];
  for (const row of rows) {
    out.push(row);
    if (row.kind === "tool" && row.output !== null && expanded.has(row.id)) {
      let b = cache.get(row.id);
      if (b?.src !== row.output) cache.set(row.id, (b = { src: row.output, items: blocks(row.id, row.output) }));
      out.push(...b.items);
    }
  }
  return out;
}

/** One line that names what a tool call did. */
export function inputSummary(input: unknown): string {
  if (input === null || input === undefined) return "";
  if (typeof input === "string") return input.slice(0, 300);
  if (typeof input === "object") {
    const o = input as Record<string, unknown>;
    for (const k of ["command", "cmd", "CommandLine", "file_path", "path", "AbsolutePath", "TargetFile", "pattern", "url"])
      if (typeof o[k] === "string") return (o[k] as string).slice(0, 300);
  }
  return JSON.stringify(input).slice(0, 300);
}
