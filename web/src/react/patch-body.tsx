// The shared patch box and layout choice for tool cards and Markdown fences.
import { Suspense, lazy, useEffect, useSyncExternalStore, type ReactNode } from "react";
import type { PatchInfo, PatchLineWire } from "../core/api";
import { store } from "./store";

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

export function ColoredRow({ kind, sourceLine, children }: { kind: PatchLineWire["kind"] | null; sourceLine?: number; children: ReactNode }) {
  return <span data-output-line={sourceLine} className={kind ? `dl ${kind}` : undefined}>{children}</span>;
}

type PierreProps = { info: Extract<PatchInfo, { lines: unknown }>; path?: string; split: boolean; wrap: boolean; previewId?: string; fileLabels: boolean };

// Pierre and its highlighter load in their own chunk the first time a patch shows. A failed load,
// such as one cut off by a reload, leaves the patch as plain text.
const PierrePatch = lazy(() => import("./pierre-patch").catch((error: unknown) => {
  console.warn("Patch colours are off:", error);
  return { default: PlainLines };
}));

function PlainLines({ info, split, fileLabels }: PierreProps) {
  const patch = info.lines.map((l) => (l.kind === "add" ? "+" : l.kind === "del" ? "-" : l.kind === "ctx" ? " " : "") + l.text).join("\n");
  return <PlainPatch patch={patch} height={patchBoxHeight(info, split, fileLabels)} />;
}

const BOX_HEIGHT = 360;

/** The height Pierre's patch box will take, from its row, separator, and header sizes, so the
 *  rows below do not move when Pierre loads. Wrapped lines are taller; the box measures them. */
export function patchBoxHeight(info: Extract<PatchInfo, { lines: unknown }>, split: boolean, fileLabels: boolean): number {
  const rows = split ? info.split.filter((r) => !("h" in r)).length : info.lines.filter((l) => l.kind === "add" || l.kind === "del" || l.kind === "ctx").length;
  // A separator shows between hunks, and above a first hunk that starts after line 1.
  let separators = 0;
  let files = 0;
  let first = true;
  info.lines.forEach((l, i) => {
    if (l.kind === "meta" && l.text.startsWith("+++ ")) { files++; first = true; }
    if (l.kind !== "hunk") return;
    const next = info.lines[i + 1];
    if (!first || (next?.old ?? 1) > 1 || (next?.new ?? 1) > 1) separators++;
    first = false;
  });
  // Each file is one item, 8 px apart: 16 px of padding, or a 44 px header and 8 px below it.
  const items = Math.max(files, 1);
  const item = fileLabels && files > 1 ? 52 : 16;
  return Math.min(BOX_HEIGHT, 16 + rows * 20 + separators * 44 + items * item + (items - 1) * 8);
}

/** The patch text, while Pierre loads or when the server sent no line layout. */
function PlainPatch({ patch, height, windowed }: { patch: string; height?: number; windowed?: boolean }) {
  const style = height === undefined ? undefined : { height, overflow: "auto", margin: 0 };
  return <pre className={`out${windowed ? " scroll-window" : ""}`} style={style}>{patch.split("\n").map((text, i, lines) => <span key={i} data-output-line={i}>{text}{i < lines.length - 1 ? "\n" : ""}</span>)}</pre>;
}

export function PatchBody({ patch, info, path, split, wrap = false, previewId, fileLabels = false, windowed }: { patch: string; info?: PatchInfo; path?: string; split: boolean; wrap?: boolean; previewId?: string; fileLabels?: boolean; windowed?: boolean }) {
  if (!info || !("lines" in info)) return <PlainPatch patch={patch} windowed={windowed} />;
  return (
    <Suspense fallback={<PlainPatch patch={patch} height={patchBoxHeight(info, split, fileLabels)} />}>
      <PierrePatch info={info} path={path} split={split} wrap={wrap} previewId={previewId} fileLabels={fileLabels} />
    </Suspense>
  );
}
