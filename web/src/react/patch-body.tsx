// The shared patch rows and layout choice for tool cards and Markdown fences.
import { useEffect, useMemo, useRef, useSyncExternalStore, type ReactNode } from "react";
import { VList, type VListHandle } from "virtua";
import type { PatchInfo, PatchLineWire } from "../core/api";
import { patchRows } from "../core/transcript";
import { store } from "./store";

const LINE_HEIGHT = 17;
const BOX_HEIGHT = 360;
const SPLIT_KEY = "swarm.diff.split";
const splitSubs = new Set<() => void>();
let splitNow = store.getItem(SPLIT_KEY) === "1";

export function setSplit(v: boolean) {
  splitNow = v;
  store.setItem(SPLIT_KEY, v ? "1" : "0");
  if (typeof document !== "undefined") document.documentElement.dataset.diffSplit = v ? "1" : "0";
  splitSubs.forEach((f) => f());
}

const subscribeSplit = (f: () => void) => {
  splitSubs.add(f);
  return () => splitSubs.delete(f);
};

export function useSplit() {
  const split = useSyncExternalStore(subscribeSplit, () => splitNow, () => false);
  useEffect(() => {
    document.documentElement.dataset.diffSplit = split ? "1" : "0";
  }, [split]);
  return split;
}

export function ColoredRow({ kind, sourceLine, children }: { kind: PatchLineWire["kind"] | null; sourceLine?: number; children: ReactNode }) {
  return <span data-output-line={sourceLine} className={kind ? `dl ${kind}` : undefined}>{children}</span>;
}

export function PatchBody({ patch, info, split, previewId, fileLabels = false }: { patch: string; info?: PatchInfo; split: boolean; previewId?: string; fileLabels?: boolean }) {
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
  const height = Math.min(BOX_HEIGHT, rows.length * LINE_HEIGHT + 4);
  return (
    <VList ref={handle} className="patch" style={{ height }} itemSize={LINE_HEIGHT} data={rows}>
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
