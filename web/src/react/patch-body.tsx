// The shared patch rows and layout choice for tool cards and Markdown fences.
import { useEffect, useMemo, useRef, useSyncExternalStore, type ReactNode } from "react";
import { VList, type CustomItemComponentProps, type VListHandle } from "virtua";
import type { PatchInfo, PatchLineWire } from "../core/api";
import { patchRows } from "../core/transcript";
import { store } from "./store";

const LINE_HEIGHT = 17;
const BOX_HEIGHT = 360;
/** One on/off diff choice: saved in the store, mirrored on <html> for static Markdown buttons. */
type Pref = { key: string; attr: "diffSplit" | "diffWrap"; now: boolean; subs: Set<() => void>; subscribe: (f: () => void) => () => void };

// Plain module-level functions: the React Compiler hoists a getter out of a hook made in a closure.
function pref(key: string, attr: Pref["attr"]): Pref {
  const subs = new Set<() => void>();
  return { key, attr, now: store.getItem(key) === "1", subs, subscribe: (f) => (subs.add(f), () => subs.delete(f)) };
}

function setPref(p: Pref, v: boolean) {
  p.now = v;
  store.setItem(p.key, v ? "1" : "0");
  if (typeof document !== "undefined") document.documentElement.dataset[p.attr] = v ? "1" : "0";
  p.subs.forEach((f) => f());
}

function usePref(p: Pref) {
  const v = useSyncExternalStore(p.subscribe, () => p.now, () => false);
  useEffect(() => {
    document.documentElement.dataset[p.attr] = v ? "1" : "0";
  }, [p, v]);
  return v;
}

const SPLIT = pref("swarm.diff.split", "diffSplit");
const WRAP = pref("swarm.diff.wrap", "diffWrap");
export const setSplit = (v: boolean) => setPref(SPLIT, v);
export const useSplit = () => usePref(SPLIT);
/** Wrap long lines in the Split layout; off, the Split layout scrolls sideways. */
export const setWrap = (v: boolean) => setPref(WRAP, v);
export const useWrap = () => usePref(WRAP);

// virtua sets `contain: layout` on each row, which hides a wide row from the box's sideways scroll.
function ScrollRow({ style, children, ref }: CustomItemComponentProps) {
  return <div ref={ref} style={{ ...style, contain: undefined }}>{children}</div>;
}

export function ColoredRow({ kind, sourceLine, children }: { kind: PatchLineWire["kind"] | null; sourceLine?: number; children: ReactNode }) {
  return <span data-output-line={sourceLine} className={kind ? `dl ${kind}` : undefined}>{children}</span>;
}

export function PatchBody({ patch, info, split, wrap = false, previewId, fileLabels = false }: { patch: string; info?: PatchInfo; split: boolean; wrap?: boolean; previewId?: string; fileLabels?: boolean }) {
  const handle = useRef<VListHandle>(null);
  const diff = info && "lines" in info ? info : undefined;
  const rows = useMemo(() => {
    if (!diff) return [];
    const rows = patchRows(diff, split);
    if (!fileLabels) return rows;
    const targets = diff.lines.flatMap((line, i) => line.kind === "meta" && line.text.startsWith("+++ ")
      ? [{ at: i, file: line.text.slice(4).replace(/^b\//, "") }] : []);
    if (targets.length < 2) return rows;
    const labeled: (typeof rows[number] | { file: string })[] = [];
    let next = 0;
    for (const row of rows) {
      const source = "i" in row ? row.i : "h" in row ? row.h : Math.min(row.l ?? Infinity, row.r ?? Infinity);
      while (next < targets.length && targets[next].at < source) labeled.push({ file: targets[next++].file });
      labeled.push(row);
    }
    while (next < targets.length) labeled.push({ file: targets[next++].file });
    return labeled;
  }, [diff, split, fileLabels]);
  // Each side is as wide as its longest line, so its column stays aligned as the box scrolls sideways.
  const sides = useMemo(() => {
    const widest = (keep: string) => (diff?.lines ?? []).reduce((w, l) => l.kind === "ctx" || l.kind === keep ? Math.max(w, l.text.replace(/\t/g, "        ").length) : w, 0);
    return split ? { "--old-ch": widest("del"), "--new-ch": widest("add") } : {};
  }, [diff, split]);
  useEffect(() => {
    if (!previewId) return;
    const find = (e: Event) => {
      const match = (e as CustomEvent<{ preview: string; line: number }>).detail;
      if (match.preview !== previewId) return;
      const i = rows.findIndex((r) => "file" in r ? false : "i" in r ? r.i === match.line : "h" in r ? r.h === match.line : r.l === match.line || r.r === match.line);
      if (i >= 0) handle.current?.scrollToIndex(i, { align: "center" });
    };
    window.addEventListener("swarm:find-output", find);
    return () => window.removeEventListener("swarm:find-output", find);
  }, [rows, previewId]);
  if (!diff) return <pre className="out">{patch.split("\n").map((text, i, lines) => <span key={i} data-output-line={i}>{text}{i < lines.length - 1 ? "\n" : ""}</span>)}</pre>;
  const wrapped = split && wrap;
  const fit = Math.min(BOX_HEIGHT, rows.length * LINE_HEIGHT + 4);
  // Wrapped rows are taller than one line, so the box grows to its content up to the cap; virtua's
  // `contain: strict` would keep it at zero height.
  const style = wrapped ? { maxHeight: BOX_HEIGHT, contain: "content" } : { height: fit, ...sides };
  return (
    <VList ref={handle} className="patch" data-wrap={wrapped ? "1" : undefined} style={style} item={ScrollRow} itemSize={LINE_HEIGHT} data={rows}>
      {(r: typeof rows[number], i: number) => {
        if ("file" in r) return <div key={i} className="pl file">{r.file}</div>;
        if ("h" in r) return <div key={i} data-output-line={r.h} className="pl hunk">{diff.lines[r.h].text}</div>;
        if ("i" in r) {
          const line = diff.lines[r.i];
          return (
            <div key={i} data-output-line={r.i} className={`pl ${line.kind}`}>
              <span className="ln">{line.old ?? ""}</span>
              <span className="ln">{line.new ?? ""}</span>
              <span className="mark">{line.kind === "add" ? "+" : line.kind === "del" ? "\u2212" : line.kind === "hunk" ? "" : " "}</span>
              <span className="code">{line.text}</span>
            </div>
          );
        }
        return (
          <div key={i} className="pl split">
            <Half line={r.l === null ? null : diff.lines[r.l]} sourceLine={r.l ?? undefined} side="old" />
            <Half line={r.r === null ? null : diff.lines[r.r]} sourceLine={r.r ?? undefined} side="new" />
          </div>
        );
      }}
    </VList>
  );
}

function Half({ line, side, sourceLine }: { line: PatchLineWire | null; side: "old" | "new"; sourceLine?: number }) {
  if (!line) return <span className="half empty" />;
  const kind = line.kind === "ctx" ? "ctx" : line.kind;
  return (
    <span data-output-line={sourceLine} className={`half ${kind}`}>
      <span className="ln">{side === "old" ? line.old : line.new}</span>
      <span className="code">{line.text}</span>
    </span>
  );
}
