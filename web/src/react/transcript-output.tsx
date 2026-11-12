// Output previews, virtual text lists, and unified or split patches.
import { CopyIcon } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { VList, type VListHandle } from "virtua";
import type { DiffPreview, PatchInfo, TextPreview } from "../core/api";
import type { FullText, Reader } from "../core/app";
import { patchOf, textRows } from "../core/transcript";
import { copyText } from "./copy";
import { ColoredRow, PatchBody, setSplit, useSplit } from "./patch-body";

const plural = (n: number, word: string) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;

/** The capped preview; Show full output fetches the whole text and shows it in a windowed box. */
function OutputBox({ app, preview, full, title }: { app: Reader; preview: TextPreview; full: ReadonlyMap<string, FullText>; title: string }) {
  const f = preview.id ? full.get(preview.id) : undefined;
  const entry = patchOf(preview);
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
        <TextLines text={f.text} entry={f.patch} previewId={preview.id} testid="full-output" />
      ) : entry && "kinds" in entry ? (
        <PatchText text={preview.text} entry={entry} />
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
function PatchText({ text, entry }: { text: string; entry?: PatchInfo }) {
  const rows = textRows(entry, text);
  return (
    <pre className="out">
      {rows.map((row, i) => (
        <ColoredRow key={i} sourceLine={i} kind={row.kind}>
          {row.text}
          {"\n"}
        </ColoredRow>
      ))}
    </pre>
  );
}

const LINE_HEIGHT = 17;
const BOX_HEIGHT = 360;

/** A windowed box of text lines, so a 200,000-line output keeps a few dozen nodes. */
function TextLines({ text, entry, previewId, testid }: { text: string; entry?: PatchInfo; previewId: string; testid?: string }) {
  const handle = useRef<VListHandle>(null);
  useEffect(() => {
    const find = (e: Event) => {
      const match = (e as CustomEvent<{ preview: string; line: number }>).detail;
      if (match.preview === previewId) handle.current?.scrollToIndex(match.line, { align: "center" });
    };
    window.addEventListener("swarm:find-output", find);
    return () => window.removeEventListener("swarm:find-output", find);
  }, [previewId]);
  const rows = useMemo(() => textRows(entry, text), [entry, text]);
  return (
    <VList ref={handle} className="out vbox" data-testid={testid} style={{ height: Math.min(BOX_HEIGHT, rows.length * LINE_HEIGHT + 12) }} itemSize={LINE_HEIGHT} data={rows}>
      {(row: ReturnType<typeof textRows>[number], i: number) => (
        <div key={i} data-output-line={i} className={row.kind ? `vline dl ${row.kind}` : "vline"}>
          {row.text || " "}
        </div>
      )}
    </VList>
  );
}

/** A patch: Unified or Split, the capped preview, Show full patch, Copy patch, and its counts. */
function DiffView({ app, diff, full }: { app: Reader; diff: DiffPreview; full: ReadonlyMap<string, FullText> }) {
  const split = useSplit();
  const f = full.get(diff.id);
  const patch = f?.state === "ready" ? f.text : diff.patch;
  const capped = diff.omitted_lines > 0 || diff.shortened_lines > 0;
  const name = diff.old_path ? `${diff.old_path} -> ${diff.path}` : diff.path.slice(diff.path.lastIndexOf("/") + 1);
  return (
    <div className="diff" data-testid="diff" data-preview={diff.id} data-split={split}>
      <div className="diff-head">
        <span className="font-medium" title={name}>
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
      <PatchBody patch={patch} info={f?.state === "ready" ? f.patch : patchOf(diff)} split={split} previewId={diff.id} />
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

export { DiffView, OutputBox, plural, setSplit };
