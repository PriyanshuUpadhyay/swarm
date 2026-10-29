import { MessageSquarePlusIcon, XIcon } from "lucide-react";
import { Fragment, memo, useMemo, useRef, useState, useSyncExternalStore, type KeyboardEvent, type MouseEvent } from "react";
import type { App } from "../core/app";
import { diffLines, type Changes, type DiffComment, type FileDiff, type Load } from "../core/changes";
import type { PatchLine } from "../core/transcript";
import { Button } from "../components/ui/button";
import { SurfaceFind } from "./surface-find";
import { textRanges, paintRanges } from "./find-text";
import { plural } from "./changes-shared";

function DiffPane({ app, changes }: { app: App; changes: Changes }) {
  const view = useSyncExternalStore(changes.subscribe, () => changes.view);
  const surface = useRef<HTMLDivElement>(null);
  const searchIndex = useRef(0);
  const search = (query: string, direction: number) => {
    const body = surface.current?.querySelector<HTMLElement>(".patch");
    const ranges = body ? textRanges(body, query) : [];
    searchIndex.current = direction ? (searchIndex.current + direction + ranges.length) % ranges.length : 0;
    const range = ranges[searchIndex.current];
    paintRanges("surface-find", range ? [range] : []);
    range?.startContainer.parentElement?.scrollIntoView({ block: "nearest" });
    return ranges.length ? `${searchIndex.current + 1} of ${ranges.length}` : query ? "No matches" : "";
  };
  const d = view.diff;
  if (d.state === "idle") return <div className="diff-pane" />;
  const data = d.state === "ready" ? d.data : null;
  return (
    <div ref={surface} tabIndex={-1} data-find-surface="diff" className="diff-pane" data-testid="diff-pane" data-state={d.state}>
      <SurfaceFind surface={surface} scope="diff" search={search} />
      {data && (
        <div className="diff-head">
          <span className="min-w-0 truncate font-medium" title={data.path} data-testid="diff-path">
            {data.path}
          </span>
          <span className="shrink-0 font-mono text-xs" data-testid="diff-counts">
            {data.added === null ? "binary" : <span className="text-done">+{data.added.toLocaleString()}</span>}{" "}
            {data.removed !== null && <span className="text-failed">−{data.removed.toLocaleString()}</span>}
          </span>
          <span className="flex-1" />
          <div className="seg" role="group" aria-label="Show">
            <button type="button" aria-pressed={!view.showFile} onClick={() => changes.setShowFile(false)}>
              Diff
            </button>
            <button type="button" aria-pressed={view.showFile} data-testid="show-file" onClick={() => changes.setShowFile(true)}>
              File
            </button>
          </div>
        </div>
      )}
      {d.state === "loading" && <p className="px-3 py-2 text-xs text-muted-foreground">Reading the diff…</p>}
      {d.state === "error" && <p className="px-3 py-2 text-xs break-words text-failed select-text">{d.error}</p>}
      {data && (view.showFile ? <FileBody file={view.file} /> : <DiffBody app={app} changes={changes} d={data} comments={view.comments} />)}
    </div>
  );
}

/** The capped patch. A click on a line number picks a line, a Shift-click picks a range; a comment
 *  box opens under it. Keys in the body: j/k or arrows move, Shift extends, Enter or c comments. */
function DiffBody({ app, changes, d, comments }: { app: App; changes: Changes; d: FileDiff; comments: DiffComment[] }) {
  const lines = useMemo(() => (d.patch ? diffLines(d.patch) : []), [d.patch]);
  const editor = useSyncExternalStore(changes.subscribe, () => changes.editor(d.path, d.layer ?? "branch"));
  const [sel, setSel] = useState<{ anchor: number; cursor: number } | null>(editor);
  const [composing, setComposing] = useState(!!editor);
  const body = useRef<HTMLDivElement>(null);
  // A new file or a new patch drops the selection.
  const identity = `${d.layer}\n${d.path}\n${d.patch}`;
  const [shown, setShown] = useState(identity);
  if (shown !== identity) {
    setShown(identity);
    setSel(editor);
    setComposing(!!editor);
  }
  if (d.notice) return <p className="px-3 py-3 text-xs text-muted-foreground">{d.notice}</p>;
  if (!lines.length) return <p className="px-3 py-3 text-xs text-muted-foreground">No text changes.</p>;

  const code = (i: number) => ["add", "del", "ctx"].includes(lines[i]?.kind);
  const lo = sel ? Math.min(sel.anchor, sel.cursor) : -1;
  const hi = sel ? Math.max(sel.anchor, sel.cursor) : -1;
  // Drafts on this file show under their last line while that line still reads the same.
  const here = new Map<number, DiffComment[]>();
  for (const c of comments)
    if (c.path === d.path && c.layer === (d.layer ?? "branch") && quote(lines[c.at]) === c.quoted[c.quoted.length - 1]) here.set(c.at, [...(here.get(c.at) ?? []), c]);

  const pick = (i: number, extend: boolean) => {
    if (!code(i)) return;
    setSel((s) => (extend && s ? { anchor: s.anchor, cursor: i } : { anchor: i, cursor: i }));
  };
  const onClick = (e: MouseEvent) => {
    const el = (e.target as Element).closest<HTMLElement>("[data-pick]");
    if (!el) return;
    pick(Number(el.dataset.pick), e.shiftKey);
    setComposing(true);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget || e.metaKey || e.ctrlKey || e.altKey) return;
    const dir = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[e.key];
    if (dir) {
      e.preventDefault();
      let i = sel ? sel.cursor + dir : dir > 0 ? 0 : lines.length - 1;
      while (i >= 0 && i < lines.length && !code(i)) i += dir;
      if (i < 0 || i >= lines.length) return;
      pick(i, e.shiftKey);
      e.currentTarget.querySelector(`[data-pick="${i}"]`)?.scrollIntoView({ block: "nearest" });
    } else if ((e.key === "Enter" || e.key === "c") && sel) {
      e.preventDefault();
      setComposing(true);
    } else if (e.key === "Escape" && sel) {
      e.preventDefault();
      setSel(null);
    }
  };
  const done = () => {
    setComposing(false);
    changes.setEditor(d.path, d.layer ?? "branch", null);
    body.current?.focus();
  };

  return (
    <>
      <div ref={body} className="patch cpatch" tabIndex={0} aria-label="Diff lines" data-testid="diff-body" onClick={onClick} onKeyDown={onKey}>
        {lines.map((l, i) => (
          <Fragment key={i}>
            <DiffLine l={l} i={i} picked={i >= lo && i <= hi} />
            {here.get(i)?.map((c) => <CommentCard key={c.id} c={c} onRemove={() => changes.removeComment(c.id)} />)}
            {composing && sel && i === hi && (
              <CommentBox
                label={`${d.path}`}
                text={editor?.text ?? ""}
                onChange={(text) => changes.setEditor(d.path, d.layer ?? "branch", { ...sel, text })}
                onAdd={(text) => {
                  changes.addComment(lo, hi, text);
                  setSel(null);
                  done();
                }}
                onCancel={done}
              />
            )}
          </Fragment>
        ))}
      </div>
      {(d.is_truncated || (d.hidden_lines ?? 0) > 0) && (
        <p className="border-t border-border px-3 py-1.5 text-xs text-muted-foreground" data-testid="diff-capped">
          Showing the first {plural(lines.length, "line")} of {plural(d.full_lines ?? lines.length, "line")}. Open the file in your editor for the rest.
        </p>
      )}
      {!composing && !sel && app.view.openId && <p className="diff-hint">Click a line number to comment; Shift-click picks a range.</p>}
    </>
  );
}

const quote = (l: PatchLine | undefined) => (l ? `${l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}${l.text}` : undefined);

const DiffLine = memo(function DiffLine({ l, i, picked }: { l: PatchLine; i: number; picked: boolean }) {
  if (l.kind === "hunk") return <div className="pl hunk">{l.text}</div>;
  return (
    <div className={`pl ${l.kind}`} data-testid="diff-line" data-picked={picked || undefined}>
      <span className="ln pick" data-pick={i} title="Comment on this line">
        {l.old ?? ""}
      </span>
      <span className="ln pick" data-pick={i} title="Comment on this line">
        {l.new ?? ""}
      </span>
      <span className="mark">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : " "}</span>
      <span className="code">{l.text}</span>
    </div>
  );
});

function CommentBox({ label, text, onChange, onAdd, onCancel }: { label: string; text: string; onChange: (text: string) => void; onAdd: (text: string) => void; onCancel: () => void }) {
  return (
    <div className="comment-box" data-testid="comment-box">
      <MessageSquarePlusIcon className="mt-1 size-3.5 shrink-0 text-muted-foreground" />
      <textarea
        autoFocus
        rows={2}
        value={text}
        aria-label={`Comment on ${label}`}
        placeholder="What should the agent do with these lines?"
        data-testid="comment-input"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            if (text.trim()) onAdd(text);
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      <div className="flex flex-col gap-1">
        <Button size="sm" data-testid="comment-add" disabled={!text.trim()} onClick={() => onAdd(text)}>
          Add
        </Button>
        <Button size="sm" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

function CommentCard({ c, onRemove }: { c: DiffComment; onRemove: () => void }) {
  return (
    <div className="comment-card" data-testid="comment-card">
      <span className="text-[11px] text-muted-foreground">{c.lines}</span>
      <span className="min-w-0 flex-1 break-words whitespace-pre-wrap">{c.text}</span>
      <button className="icon-btn" aria-label="Remove comment" onClick={onRemove}>
        <XIcon />
      </button>
    </div>
  );
}

function FileBody({ file }: { file: Load<{ path: string; preview?: { text: string; full_lines: number; hidden_lines: number }; notice?: string }> }) {
  if (file.state === "loading" || file.state === "idle") return <p className="px-3 py-2 text-xs text-muted-foreground">Reading the file…</p>;
  if (file.state === "error") return <p className="px-3 py-2 text-xs break-words text-failed select-text">{file.error}</p>;
  const p = file.data.preview;
  if (!p) return <p className="px-3 py-3 text-xs text-muted-foreground">{file.data.notice}</p>;
  const lines = p.text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return (
    <>
      <div className="patch cpatch" data-testid="file-body">
        {lines.map((t, i) => (
          <div key={i} className="pl ctx">
            <span className="ln">{i + 1}</span>
            <span className="code">{t}</span>
          </div>
        ))}
      </div>
      {p.hidden_lines > 0 && <p className="border-t border-border px-3 py-1.5 text-xs text-muted-foreground">Showing the first {plural(lines.length, "line")} of {plural(p.full_lines, "line")}.</p>}
    </>
  );
}

export { DiffPane };
