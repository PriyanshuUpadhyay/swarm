// Patches painted by Pierre: the shared theme options, the patch rebuilt from the server's display
// lines, and the virtual patch box for tool cards and Markdown fences.
import { CodeView, type CodeViewHandle } from "@pierre/diffs/react";
import { parsePatchFiles, type CodeViewDiffItem, type SelectedLineRange } from "@pierre/diffs";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { PatchInfo, PatchLineWire } from "../core/api";
import { patchBoxHeight } from "./patch-body";
import { addFinder } from "./patch-find";

// Pierre renders each diff in a shadow root. Find paints its ranges there, so the shadow root needs
// the page's highlight colours too.
export const PIERRE = {
  theme: { dark: "pierre-dark", light: "pierre-light" },
  unsafeCSS: [
    "::highlight(surface-find) { background: #ff963b; color: #202020; }",
    "::highlight(find-matches) { background: #f5da62; color: #202020; }",
    "::highlight(find-current) { background: #ff963b; color: #202020; }",
  ].join("\n"),
} as const;

export type Side = SelectedLineRange["side"] & string;

/** Where Pierre shows a display line: deleted lines on the old side, the rest on the new side. */
export function spotOf(l: PatchLineWire | undefined): { side: Side; lineNumber: number } | null {
  if (l?.kind === "del" && l.old != null) return { side: "deletions", lineNumber: l.old };
  if ((l?.kind === "add" || l?.kind === "ctx") && l.new != null) return { side: "additions", lineNumber: l.new };
  return null;
}

export function indexOf(lines: PatchLineWire[], side: Side | undefined, n: number): number {
  return lines.findIndex((l) => side === "deletions" ? (l.kind === "del" || l.kind === "ctx") && l.old === n : (l.kind === "add" || l.kind === "ctx") && l.new === n);
}

/** A unified patch built from the server's display lines, so Pierre paints the same lines that
 *  comments and Find point at. Each hunk header is rebuilt from the line numbers, because a capped
 *  patch can end inside a hunk. A `+++` line starts the next file. */
export function patchText(lines: PatchLineWire[], path = "patch"): string {
  const out: string[] = [];
  let run: PatchLineWire[] = [];
  let old = path;
  const flush = () => {
    if (!run.length) return;
    const olds = run.flatMap((l) => l.kind !== "add" && l.old != null ? [l.old] : []);
    const news = run.flatMap((l) => l.kind !== "del" && l.new != null ? [l.new] : []);
    out.push(`@@ -${olds.length ? Math.min(...olds) : 0},${olds.length} +${news.length ? Math.min(...news) : 0},${news.length} @@`);
    for (const l of run) out.push((l.kind === "add" ? "+" : l.kind === "del" ? "-" : " ") + l.text);
    run = [];
  };
  const file = (from: string, to: string) => {
    flush();
    // Pierre strips the a/ and b/ prefixes only under a git header.
    out.push(`diff --git a/${from} b/${to}`, `--- a/${from}`, `+++ b/${to}`);
  };
  for (const l of lines) {
    if (l.kind === "meta" && l.text.startsWith("--- ")) old = l.text.slice(4).replace(/^a\//, "");
    else if (l.kind === "meta" && l.text.startsWith("+++ ")) file(old, l.text.slice(4).replace(/^b\//, ""));
    else if (l.kind === "hunk") flush();
    else if (spotOf(l)) {
      if (!out.length) file(path, path);
      run.push(l);
    }
  }
  flush();
  return out.join("\n") + "\n";
}

/** The file each display line belongs to, counted the way `patchText` starts files. */
function fileOfLines(lines: PatchLineWire[]): number[] {
  let file = -1;
  let started = false;
  return lines.map((l) => {
    if (l.kind === "meta" && l.text.startsWith("+++ ")) { file++; started = true; }
    else if (!started && spotOf(l)) { file = 0; started = true; }
    return Math.max(file, 0);
  });
}

const BOX_HEIGHT = 360;

/** A patch in a box up to 360 px tall. Pierre's CodeView draws only the rows in view, so a
 *  20,000-line full patch keeps a few dozen rows, and it scrolls a Find match into view. */
export default function PierrePatch({ info, path, split, wrap, previewId, fileLabels }: { info: Extract<PatchInfo, { lines: unknown }>; path?: string; split: boolean; wrap: boolean; previewId?: string; fileLabels: boolean }) {
  const view = useRef<CodeViewHandle<undefined, undefined>>(null);
  const box = useRef<HTMLDivElement>(null);
  const items = useMemo<CodeViewDiffItem[]>(
    () => parsePatchFiles(patchText(info.lines, path)).flatMap((p) => p.files).map((fileDiff, i) => ({ id: String(i), type: "diff", fileDiff })),
    [info, path],
  );
  const files = useMemo(() => fileOfLines(info.lines), [info]);
  const headers = fileLabels && items.length > 1;
  // The guess matches the loading box; the measured content height replaces it.
  const [height, setHeight] = useState(() => patchBoxHeight(info, split, fileLabels));
  useLayoutEffect(() => {
    const root = box.current;
    const content = root?.firstElementChild;
    if (!root || !content) return;
    // The content's margins hold CodeView's top and bottom padding.
    const fit = () => {
      const style = getComputedStyle(content);
      setHeight(Math.min(BOX_HEIGHT, content.getBoundingClientRect().height + parseFloat(style.marginTop) + parseFloat(style.marginBottom)));
    };
    const watch = new ResizeObserver(fit);
    watch.observe(content);
    return () => watch.disconnect();
  }, [items]);
  useEffect(() => {
    if (!previewId) return;
    // The first drawable line at or after a display line; a hunk header points at its first line.
    const target = (line: number) => {
      let i = line;
      while (i < info.lines.length && !spotOf(info.lines[i])) i++;
      const spot = spotOf(info.lines[i]);
      return spot && { id: String(files[i]), spot, kind: info.lines[i].kind };
    };
    const scroll = (e: Event) => {
      const match = (e as CustomEvent<{ preview: string; line: number }>).detail;
      const t = match.preview === previewId ? target(match.line) : null;
      if (t) view.current?.scrollTo({ type: "line", id: t.id, ...t.spot, align: "center", behavior: "instant" });
    };
    window.addEventListener("swarm:find-output", scroll);
    const stop = addFinder(previewId, (line) => {
      const t = target(line);
      const item = t && view.current?.getInstance()?.getRenderedItems().find((r) => r.id === t.id);
      if (!t || !item) return null;
      const n = t.spot.lineNumber;
      // Split puts each side in its own column. Unified numbers a context line by its new side.
      const row = split
        ? `code[data-${t.spot.side}] [data-content] > [data-line="${n}"]`
        : t.kind === "del" ? `[data-content] > [data-line="${n}"][data-line-type="change-deletion"]` : `[data-content] > [data-line="${n}"]:not([data-line-type="change-deletion"])`;
      return item.element.shadowRoot?.querySelector(row) ?? null;
    });
    return () => {
      window.removeEventListener("swarm:find-output", scroll);
      stop();
    };
  }, [info, files, previewId, split]);
  return (
    <CodeView
      ref={view}
      containerRef={box}
      className="patch"
      style={{ height }}
      items={items}
      disableWorkerPool
      options={{
        ...PIERRE,
        diffStyle: split ? "split" : "unified",
        overflow: split && wrap ? "wrap" : "scroll",
        disableFileHeader: !headers,
        hunkSeparators: "line-info",
      }}
    />
  );
}
