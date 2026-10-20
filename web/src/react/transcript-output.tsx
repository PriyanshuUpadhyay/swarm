// Output previews, virtual text lists, and unified or split patches.
import { CopyIcon } from "lucide-react";
import { useMemo, useSyncExternalStore } from "react";
import { VList } from "virtua";
import type { DiffPreview, TextPreview } from "../core/api";
import type { FullText, Reader } from "../core/app";
import { isPatch, parsePatch, patchKinds, splitPatch, type PatchLine, type SplitRow } from "../core/transcript";
import { copyText } from "./copy";
import { store } from "./store";

const plural = (n: number, word: string) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;

/** The capped preview; Show full output fetches the whole text and shows it in a windowed box. */
function OutputBox({ app, preview, full, title }: { app: Reader; preview: TextPreview; full: ReadonlyMap<string, FullText>; title: string }) {
  const f = preview.id ? full.get(preview.id) : undefined;
  const copy = () => copyText(preview.is_truncated ? app.fullText(preview.id) : preview.text, title.toLowerCase());
  return (
    <div className="grid gap-1" data-testid="output">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <span className="font-medium">{f?.state === "ready" ? `${title} · ${plural(f.lines.length, "line")}` : title}</span>
        <span className="flex-1" />
        <button type="button" className="t-link" onClick={copy}>
          <CopyIcon className="size-3" /> Copy {title.toLowerCase()}
        </button>
      </div>
      {f?.state === "ready" ? (
        <TextLines lines={f.lines} testid="full-output" />
      ) : isPatch(preview.text) ? (
        <PatchText text={preview.text} />
      ) : (
        <pre className="out">{preview.text || "No output was recorded."}</pre>
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
        <span key={i} className={`dl ${kinds[i]}`}>
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
function TextLines({ lines, testid }: { lines: string[]; testid?: string }) {
  const kinds = useMemo(() => (lines.some((l) => l.startsWith("@@ ")) ? patchKinds(lines) : null), [lines]);
  return (
    <VList className="out vbox" data-testid={testid} style={{ height: Math.min(BOX_HEIGHT, lines.length * LINE_HEIGHT + 12) }} itemSize={LINE_HEIGHT} data={lines}>
      {(text: string, i: number) => (
        <div key={i} className={kinds ? `vline dl ${kinds[i]}` : "vline"}>
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
    <div className="diff" data-testid="diff" data-split={split}>
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
        <button type="button" className="t-link" onClick={() => copyText(capped ? app.fullText(diff.id) : diff.patch, "patch")}>
          <CopyIcon className="size-3" /> Copy patch
        </button>
      </div>
      <PatchBody patch={patch} split={split} />
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

function PatchBody({ patch, split }: { patch: string; split: boolean }) {
  const parsed = useMemo(() => parsePatch(patch), [patch]);
  const rows: (PatchLine | SplitRow)[] = split ? splitPatch(parsed) : parsed.filter((l) => l.kind !== "meta");
  const height = Math.min(BOX_HEIGHT, rows.length * LINE_HEIGHT + 4);
  return (
    <VList className="patch" style={{ height }} itemSize={LINE_HEIGHT} data={rows}>
      {(r: PatchLine | SplitRow, i: number) => {
        if ("hunk" in r) return <div key={i} className="pl hunk">{r.hunk}</div>;
        if ("kind" in r)
          return (
            <div key={i} className={`pl ${r.kind}`}>
              <span className="ln">{r.old ?? ""}</span>
              <span className="ln">{r.new ?? ""}</span>
              <span className="mark">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : r.kind === "hunk" ? "" : " "}</span>
              <span className="code">{r.text}</span>
            </div>
          );
        return (
          <div key={i} className="pl split">
            <Half line={r.left} side="old" />
            <Half line={r.right} side="new" />
          </div>
        );
      }}
    </VList>
  );
}

function Half({ line, side }: { line: PatchLine | null; side: "old" | "new" }) {
  if (!line) return <span className="half empty" />;
  const kind = line.kind === "ctx" ? "ctx" : line.kind;
  return (
    <span className={`half ${kind}`}>
      <span className="ln">{side === "old" ? line.old : line.new}</span>
      <span className="code">{line.text}</span>
    </span>
  );
}

export { DiffView, OutputBox, plural, setSplit };
