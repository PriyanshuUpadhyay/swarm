import { MessageSquarePlusIcon, XIcon } from "lucide-react";
import { File, PatchDiff, type DiffLineAnnotation, type FileDiffOptions, type FileOptions, type SelectedLineRange } from "@pierre/diffs/react";
import { useMemo, useRef, useState, useSyncExternalStore, type KeyboardEvent } from "react";
import type { App } from "../core/app";
import { type Changes, type DiffComment, type FileDiff, type Load } from "../core/changes";
import { Button } from "../components/ui/button";
import { SurfaceFind } from "./surface-find";
import { textRanges, paintRanges } from "./find-text";
import { PIERRE, indexOf, patchText, spotOf } from "./pierre-patch";
import { plural } from "./changes-shared";

function DiffPane({ app, changes }: { app: App; changes: Changes }) {
  const view = useSyncExternalStore(changes.subscribe, () => changes.view);
  const surface = useRef<HTMLDivElement>(null);
  const searchIndex = useRef(0);
  const search = (query: string, direction: number) => {
    const body = surface.current?.querySelector("diffs-container")?.shadowRoot;
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
          <span className="diff-name font-medium" title={data.path} data-testid="diff-path">
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

type Note = { at: number; comments: DiffComment[]; box: boolean };

/** The capped patch. A click on a line number picks a line, a Shift-click picks a range; a comment
 *  box opens under it. Keys in the body: j/k or arrows move, Shift extends, Enter or c comments. */
function DiffBody({ app, changes, d, comments }: { app: App; changes: Changes; d: FileDiff; comments: DiffComment[] }) {
  const lines = d.display?.lines ?? [];
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

  const code = (i: number) => spotOf(lines[i]) !== null;
  const lo = sel ? Math.min(sel.anchor, sel.cursor) : -1;
  const hi = sel ? Math.max(sel.anchor, sel.cursor) : -1;
  const pick = (next: { anchor: number; cursor: number } | null) => {
    setSel(next);
    changes.setEditor(d.path, d.layer ?? "branch", next && { ...next, text: editor?.text ?? "" });
  };
  const options: FileDiffOptions<Note, undefined> = {
    ...PIERRE,
    diffStyle: "unified",
    disableFileHeader: true,
    enableLineSelection: true,
    controlledSelection: true,
    onLineSelected: (range) => {
      const anchor = range ? indexOf(lines, range.side, range.start) : -1;
      const cursor = range ? indexOf(lines, range.endSide ?? range.side, range.end) : -1;
      if (anchor < 0 || cursor < 0) return pick(null);
      pick({ anchor, cursor });
      setComposing(true);
    },
  };
  const patch = useMemo(() => patchText(lines, d.path), [d.path, lines]);
  const selected = useMemo((): SelectedLineRange | null => {
    const a = sel && spotOf(lines[sel.anchor]);
    const c = sel && spotOf(lines[sel.cursor]);
    return a && c ? { start: a.lineNumber, side: a.side, end: c.lineNumber, endSide: c.side } : null;
  }, [sel, lines]);
  // Drafts on this file show under their last line while that line still reads the same.
  const notes = useMemo(() => {
    const at = new Map<number, Note>();
    const note = (i: number) => at.get(i) ?? at.set(i, { at: i, comments: [], box: false }).get(i)!;
    for (const c of comments)
      if (c.path === d.path && c.layer === (d.layer ?? "branch") && lines[c.at]?.quote === c.quoted[c.quoted.length - 1]) note(c.at).comments.push(c);
    if (composing && hi >= 0) note(hi).box = true;
    return [...at.values()].flatMap((n): DiffLineAnnotation<Note>[] => {
      const spot = spotOf(lines[n.at]);
      return spot ? [{ ...spot, metadata: n }] : [];
    });
  }, [comments, d.path, d.layer, lines, composing, hi]);

  if (d.notice) return <p className="px-3 py-3 text-xs text-muted-foreground">{d.notice}</p>;
  if (!d.display || !d.patch) return <pre className="patch cpatch" data-testid="diff-body">{d.patch ?? ""}</pre>;
  if (!lines.length) return <p className="px-3 py-3 text-xs text-muted-foreground">No text changes.</p>;

  const onKey = (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget || e.metaKey || e.ctrlKey || e.altKey) return;
    const dir = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[e.key];
    if (dir) {
      e.preventDefault();
      let i = sel ? sel.cursor + dir : dir > 0 ? 0 : lines.length - 1;
      while (i >= 0 && i < lines.length && !code(i)) i += dir;
      if (i < 0 || i >= lines.length) return;
      pick(e.shiftKey && sel ? { anchor: sel.anchor, cursor: i } : { anchor: i, cursor: i });
      const spot = spotOf(lines[i])!;
      const type = spot.side === "deletions" ? '[data-line-type="change-deletion"]' : ':not([data-line-type="change-deletion"])';
      e.currentTarget.querySelector("diffs-container")?.shadowRoot?.querySelector(`[data-content] > [data-line="${spot.lineNumber}"]${type}`)?.scrollIntoView({ block: "nearest" });
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
  const renderNote = ({ metadata: n }: DiffLineAnnotation<Note>) => (
    <>
      {n.comments.map((c) => <CommentCard key={c.id} c={c} onRemove={() => changes.removeComment(c.id)} />)}
      {n.box && sel && (
        <CommentBox
          label={`${d.path}`}
          text={editor?.text ?? ""}
          onChange={(text) => changes.setEditor(d.path, d.layer ?? "branch", { ...sel, text })}
          onAdd={async (text) => {
            const captured = changes.editor(d.path, d.layer ?? "branch");
            const success = await changes.addComment(lo, hi, text);
            if (success && changes.editor(d.path, d.layer ?? "branch") === captured) {
              setSel(null);
              done();
            }
            return success;
          }}
          onCancel={done}
        />
      )}
    </>
  );

  return (
    <>
      <div ref={body} className="cpatch" tabIndex={0} aria-label="Diff lines" data-testid="diff-body" onKeyDown={onKey}>
        <PatchDiff<Note, undefined> patch={patch} options={options} selectedLines={selected} lineAnnotations={notes} renderAnnotation={renderNote} disableWorkerPool />
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

function CommentBox({ label, text, onChange, onAdd, onCancel }: { label: string; text: string; onChange: (text: string) => void; onAdd: (text: string) => Promise<boolean>; onCancel: () => void }) {
  const [pending, setPending] = useState(false);
  const submitting = useRef(false);
  const add = async () => {
    if (submitting.current || !text.trim()) return;
    submitting.current = true;
    setPending(true);
    try {
      await onAdd(text);
    } finally {
      submitting.current = false;
      setPending(false);
    }
  };
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
            void add();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      <div className="flex flex-col gap-1">
        <Button size="sm" data-testid="comment-add" disabled={pending || !text.trim()} onClick={() => void add()}>
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

const FILE_OPTIONS: FileOptions<undefined, undefined> = { ...PIERRE, disableFileHeader: true };

function FileBody({ file }: { file: Load<{ path: string; preview?: { text: string; full_lines: number; hidden_lines: number }; notice?: string }> }) {
  if (file.state === "loading" || file.state === "idle") return <p className="px-3 py-2 text-xs text-muted-foreground">Reading the file…</p>;
  if (file.state === "error") return <p className="px-3 py-2 text-xs break-words text-failed select-text">{file.error}</p>;
  const p = file.data.preview;
  if (!p) return <p className="px-3 py-3 text-xs text-muted-foreground">{file.data.notice}</p>;
  const shown = p.text.endsWith("\n") ? p.text.split("\n").length - 1 : p.text.split("\n").length;
  return (
    <>
      <div className="cpatch" data-testid="file-body">
        <File file={{ name: file.data.path, contents: p.text }} options={FILE_OPTIONS} disableWorkerPool />
      </div>
      {p.hidden_lines > 0 && <p className="border-t border-border px-3 py-1.5 text-xs text-muted-foreground">Showing the first {plural(shown, "line")} of {plural(p.full_lines, "line")}.</p>}
    </>
  );
}

export { DiffPane };
