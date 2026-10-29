// Output previews, virtual text lists, and unified or split patches.
import { CopyIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import { VList, type VListHandle } from "virtua";
import type { DiffPreview, TextPreview } from "../core/api";
import type { FullText, Reader } from "../core/app";
import { isPatch, parsePatch, patchKinds, splitPatch, type PatchLine, type SplitRow } from "../core/transcript";
import { copyText } from "./copy";
import { store } from "./store";

const plural = (n: number, word: string) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;

/** The capped preview; Show full output fetches the whole text and shows it in a windowed box. */
function OutputBox({ app, preview, full, title }: { app: Reader; preview: TextPreview; full: ReadonlyMap<string, FullText>; title: string }) {
  const f = preview.id ? full.get(preview.id) : undefined;
  const copy = (e: React.MouseEvent) => copyText(preview.is_truncated ? app.fullText(preview.id) : preview.text, title.toLowerCase(), e.currentTarget);
  return (
    <div className="grid gap-1" data-testid="output" data-preview={preview.id}>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <span className="font-medium">{f?.state === "ready" ? `${title} · ${plural(f.lines.length, "line")}` : title}</span>
        <span className="flex-1" />
        <button type="button" className="t-link" onClick={copy}>
          <CopyIcon className="size-3" /> Copy {title.toLowerCase()}
        </button>
      </div>
      {f?.state === "ready" ? (
        <TextLines lines={f.lines} previewId={preview.id} testid="full-output" />
      ) : isPatch(preview.text) ? (
        <PatchText text={preview.text} />
      ) : (
        <pre className="out">{(preview.text || "No output was recorded.").split("\n").map((text, i, lines) => <span key={i} data-output-line={i}>{text}{i < lines.length - 1 ? "\n" : ""}</span>)}</pre>
      )}
      {preview.is_truncated && f?.state !== "ready" && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>… +{plural(preview.hidden_lines, "line")}</span>
          <button type="button" className="t-link" data-testid="show-full" disabled={f?.state === "loading"} onClick={() => void app.loadFull(preview.id)}>
            {f?.state === "loading" ? "Loading full output…" : "Show full output"}
          </button>
          {f?.state === "error" && <span className="text-failed">{f.error}</span>}
        </div>
      )}
    </div>
  );
}

/** Text that holds a unified diff, such as `diff -u` output, with each line in its diff colour. */
function PatchText({ text }: { text: string }) {
  const lines = text.split("\n");
  const kinds = patchKinds(lines);
  return (
    <pre className="out">
      {lines.map((l, i) => (
        <span key={i} data-output-line={i} className={`dl ${kinds[i]}`}>
          {l}
          {"\n"}
        </span>
      ))}
    </pre>
  );
}

const LINE_HEIGHT = 17;
const BOX_HEIGHT = 360;

/** A windowed box of text lines, so a 200,000-line output keeps a few dozen nodes. */
function TextLines({ lines, previewId, testid }: { lines: string[]; previewId: string; testid?: string }) {
  const handle = useRef<VListHandle>(null);
  useEffect(() => {
    const find = (e: Event) => {
      const match = (e as CustomEvent<{ preview: string; line: number }>).detail;
      if (match.preview === previewId) handle.current?.scrollToIndex(match.line, { align: "center" });
    };
    window.addEventListener("swarm:find-output", find);
    return () => window.removeEventListener("swarm:find-output", find);
  }, [previewId]);
  const kinds = useMemo(() => (lines.some((l) => l.startsWith("@@ ")) ? patchKinds(lines) : null), [lines]);
  return (
    <VList ref={handle} className="out vbox" data-testid={testid} style={{ height: Math.min(BOX_HEIGHT, lines.length * LINE_HEIGHT + 12) }} itemSize={LINE_HEIGHT} data={lines}>
      {(text: string, i: number) => (
        <div key={i} data-output-line={i} className={kinds ? `vline dl ${kinds[i]}` : "vline"}>
          {text || " "}
        </div>
      )}
    </VList>
  );
}

// One Unified/Split choice for every diff, kept across sessions. The markdown diff fences are
// static HTML, so CSS reads the choice from `data-diff-split` on <html>.
const SPLIT_KEY = "swarm.diff.split";
const splitSubs = new Set<() => void>();
let splitNow = store.getItem(SPLIT_KEY) === "1";
document.documentElement.dataset.diffSplit = splitNow ? "1" : "0";

function setSplit(v: boolean) {
  splitNow = v;
  store.setItem(SPLIT_KEY, v ? "1" : "0");
  document.documentElement.dataset.diffSplit = v ? "1" : "0";
  splitSubs.forEach((f) => f());
}

const subscribeSplit = (f: () => void) => {
  splitSubs.add(f);
  return () => splitSubs.delete(f);
};

/** A patch: Unified or Split, the capped preview, Show full patch, Copy patch, and its counts. */
function DiffView({ app, diff, full }: { app: Reader; diff: DiffPreview; full: ReadonlyMap<string, FullText> }) {
  const split = useSyncExternalStore(subscribeSplit, () => splitNow);
  const f = full.get(diff.id);
  const patch = f?.state === "ready" ? f.text : diff.patch;
  const capped = diff.omitted_lines > 0 || diff.shortened_lines > 0;
  const name = diff.path.slice(diff.path.lastIndexOf("/") + 1);
  return (
    <div className="diff" data-testid="diff" data-preview={diff.id} data-split={split}>
      <div className="diff-head">
        <span className="font-medium" title={diff.path}>
          {name}
        </span>
        <span className="font-mono text-xs">
          <span className="text-done">+{diff.added}</span> <span className="text-failed">−{diff.removed}</span>
        </span>
        <span className="flex-1" />
        <div className="seg" role="group" aria-label="Diff layout">
          <button type="button" aria-pressed={!split} onClick={() => setSplit(false)}>
            Unified
          </button>
          <button type="button" aria-pressed={split} onClick={() => setSplit(true)}>
            Split
          </button>
        </div>
        <button type="button" className="t-link" onClick={(e) => copyText(capped ? app.fullText(diff.id) : diff.patch, "patch", e.currentTarget)}>
          <CopyIcon className="size-3" /> Copy patch
        </button>
      </div>
      <PatchBody patch={patch} split={split} previewId={diff.id} />
      {capped && f?.state !== "ready" && (
        <div className="flex items-center gap-2 px-2 pb-1.5 text-xs text-muted-foreground">
          <span>
            Showing a preview{diff.omitted_lines ? `, ${plural(diff.omitted_lines, "line")} left out` : ""}
            {diff.shortened_lines ? `, ${plural(diff.shortened_lines, "long line")} cut` : ""}.
          </span>
          <button type="button" className="t-link" disabled={f?.state === "loading"} onClick={() => void app.loadFull(diff.id)}>
            {f?.state === "loading" ? "Loading full patch…" : "Show full patch"}
          </button>
          {f?.state === "error" && <span className="text-failed">{f.error}</span>}
        </div>
      )}
    </div>
  );
}

function PatchBody({ patch, split, previewId }: { patch: string; split: boolean; previewId: string }) {
  const handle = useRef<VListHandle>(null);
  const parsed = useMemo(() => parsePatch(patch), [patch]);
  const sourceLines = useMemo(() => {
    const raw = patch.split("\n");
    const positions = new Map<PatchLine, number>();
    let i = 0;
    for (const line of parsed) {
      const text = line.kind === "add" ? `+${line.text}` : line.kind === "del" ? `-${line.text}` : line.kind === "ctx" ? ` ${line.text}` : line.text;
      while (i < raw.length && raw[i] !== text) i++;
      positions.set(line, i++);
    }
    return positions;
  }, [patch, parsed]);
  const rows: (PatchLine | SplitRow)[] = split ? splitPatch(parsed) : parsed.filter((l) => l.kind !== "meta");
  useEffect(() => {
    const find = (e: Event) => {
      const match = (e as CustomEvent<{ preview: string; line: number }>).detail;
      if (match.preview !== previewId) return;
      const target = parsed.find((line) => sourceLines.get(line) === match.line);
      if (!target) return;
      const same = (line: PatchLine | null) => line === target;
      const i = rows.findIndex((r) => "kind" in r ? same(r) : "hunk" in r ? r.hunk === target.text : same(r.left) || same(r.right));
      if (i >= 0) handle.current?.scrollToIndex(i, { align: "center" });
    };
    window.addEventListener("swarm:find-output", find);
    return () => window.removeEventListener("swarm:find-output", find);
  }, [patch, split, previewId, sourceLines]);
  const height = Math.min(BOX_HEIGHT, rows.length * LINE_HEIGHT + 4);
  return (
    <VList ref={handle} className="patch" style={{ height }} itemSize={LINE_HEIGHT} data={rows}>
      {(r: PatchLine | SplitRow, i: number) => {
        if ("hunk" in r) return <div key={i} data-output-line={sourceLines.get(parsed.find((line) => line.kind === "hunk" && line.text === r.hunk)!)} className="pl hunk">{r.hunk}</div>;
        if ("kind" in r)
          return (
            <div key={i} data-output-line={sourceLines.get(r)} className={`pl ${r.kind}`}>
              <span className="ln">{r.old ?? ""}</span>
              <span className="ln">{r.new ?? ""}</span>
              <span className="mark">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : r.kind === "hunk" ? "" : " "}</span>
              <span className="code">{r.text}</span>
            </div>
          );
        return (
          <div key={i} className="pl split">
            <Half line={r.left} sourceLine={r.left ? sourceLines.get(r.left) : undefined} side="old" />
            <Half line={r.right} sourceLine={r.right ? sourceLines.get(r.right) : undefined} side="new" />
          </div>
        );
      }}
    </VList>
  );
}

function Half({ line, side, sourceLine }: { line: PatchLine | null; side: "old" | "new"; sourceLine?: number }) {
  if (!line) return <span className="half empty" />;
  const kind = line.kind === "ctx" ? "ctx" : line.kind;
  return (
    <span data-output-line={sourceLine} className={`half ${kind}`}>
      <span className="ln">{side === "old" ? line.old : line.new}</span>
      <span className="code">{line.text}</span>
    </span>
  );
}

export { DiffView, OutputBox, plural, setSplit };
