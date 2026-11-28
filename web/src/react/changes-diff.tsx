import { MessageSquarePlusIcon, XIcon } from "lucide-react";
import { CodeView, File, PatchDiff, type CodeViewHandle, type DiffLineAnnotation, type FileDiffOptions, type FileOptions, type SelectedLineRange } from "@pierre/diffs/react";
import { parsePatchFiles, type CodeViewDiffItem } from "@pierre/diffs";
import { useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type KeyboardEvent, type RefObject } from "react";
import type { App } from "../core/app";
import { type Changes, type DiffComment, type FileDiff, type Load } from "../core/changes";
import { Button } from "../components/ui/button";
import { SurfaceFind } from "./surface-find";
import { textRangeAt, textRanges, paintRanges } from "./find-text";
import { PIERRE, indexOf, patchText, spotOf } from "./pierre-patch";
import { plural } from "./changes-shared";
import { CutLine } from "./cut-line";

const ITEM_ID = "patch";
const DIFF_OPTIONS = {
  ...PIERRE,
  unsafeCSS: `${PIERRE.unsafeCSS}
    [data-content] > [data-line][data-selected-line], [data-gutter] > [data-column-number][data-selected-line] { background: var(--selected); }
    [data-gutter] > [data-column-number][data-selected-line] { box-shadow: inset 2px 0 var(--foreground); }`,
  diffStyle: "unified",
  disableFileHeader: true,
  enableLineSelection: true,
} as const;
type WindowRef = RefObject<CodeViewHandle<Note, undefined> | null>;

function rowAt(root: Element | null, spot: NonNullable<ReturnType<typeof spotOf>>) {
  const type = spot.side === "deletions" ? '[data-line-type="change-deletion"]' : ':not([data-line-type="change-deletion"])';
  return root?.querySelector("diffs-container")?.shadowRoot?.querySelector(`[data-content] > [data-line="${spot.lineNumber}"]${type}`);
}

function DiffPane({ app, changes }: { app: App; changes: Changes }) {
  const view = useSyncExternalStore(changes.subscribe, () => changes.view);
  const surface = useRef<HTMLDivElement>(null);
  const window = useRef<CodeViewHandle<Note, undefined>>(null);
  const searchIndex = useRef(0);
  const paint = useRef(0);
  const d = view.diff;
  const data = d.state === "ready" ? d.data : null;
  const full = view.full.state === "ready";
  const search = (query: string, direction: number) => {
    cancelAnimationFrame(paint.current);
    const q = query.trim().toLowerCase();
    const move = (count: number) => searchIndex.current = direction && count ? (searchIndex.current + direction + count) % count : 0;

    if (full && !view.showFile && data?.display) {
      const hits: { spot: NonNullable<ReturnType<typeof spotOf>>; first: number; text: string }[] = [];
      let count = 0;
      if (q) for (const line of data.display.lines) {
        const spot = spotOf(line);
        if (!spot) continue;
        const text = line.text.toLowerCase();
        let at = text.indexOf(q);
        if (at < 0) continue;
        hits.push({ spot, first: count, text });
        for (; at >= 0; at = text.indexOf(q, at + q.length)) count++;
      }

      const i = move(count);
      const hit = hits.reverse().find(({ first }) => first <= i);
      paintRanges("surface-find", []);

      if (hit) {
        let offset = -q.length;
        for (let n = hit.first; n <= i; n++) offset = hit.text.indexOf(q, offset + q.length);
        const viewer = window.current;
        viewer?.scrollTo({ type: "line", id: ITEM_ID, ...hit.spot, align: "center", behavior: "instant" });
        paint.current = requestAnimationFrame(() => {
          const row = viewer === window.current && rowAt(surface.current, hit.spot);
          const range = row ? textRangeAt(row, offset, q.length) : undefined;
          if (range) paintRanges("surface-find", [range]);
        });
      }
      return count ? `${i + 1} of ${count}` : q ? "No matches" : "";
    }

    const body = surface.current?.querySelector("diffs-container")?.shadowRoot;
    const ranges = body ? textRanges(body, query) : [];
    const i = move(ranges.length);
    const range = ranges[i];
    paintRanges("surface-find", range ? [range] : []);
    range?.startContainer.parentElement?.scrollIntoView({ block: "nearest" });
    return ranges.length ? `${i + 1} of ${ranges.length}` : q ? "No matches" : "";
  };
  if (d.state === "idle") return <div className="diff-pane" />;
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
      {data && (view.showFile ? <FileBody file={view.file} /> : <DiffBody app={app} changes={changes} d={data} comments={view.comments} expanded={view.full} window={window} />)}
    </div>
  );
}

type Note = { at: number; comments: DiffComment[]; box: boolean };

/** Click to pick a line, Shift-click to pick a range. Keys: j/k or arrows move, Shift extends,
 *  Enter or c comments. A full patch uses the same indexes but only mounts the rows in view. */
function DiffBody({ app, changes, d, comments, expanded, window }: { app: App; changes: Changes; d: FileDiff; comments: DiffComment[]; expanded: Load<FileDiff>; window: WindowRef }) {
  const full = expanded.state === "ready";
  const lines = d.display?.lines ?? [];
  const editor = useSyncExternalStore(changes.subscribe, () => changes.editor(d.path, d.layer ?? "branch"));
  const [sel, setSel] = useState<{ anchor: number; cursor: number } | null>(editor);
  const composing = !!editor?.open;
  const body = useRef<HTMLDivElement>(null);
  // A new file or a new patch drops the selection.
  const identity = `${d.layer}\n${d.path}\n${d.patch}`;
  const [shown, setShown] = useState(identity);
  if (shown !== identity) {
    setShown(identity);
    setSel(editor);
  }

  const code = (i: number) => spotOf(lines[i]) !== null;
  const lo = sel ? Math.min(sel.anchor, sel.cursor) : -1;
  const hi = sel ? Math.max(sel.anchor, sel.cursor) : -1;

  const pick = (next: { anchor: number; cursor: number } | null, open = composing) => {
    if (changes.addingComment) return;
    setSel(next);
    const layer = d.layer ?? "branch";
    const text = editor?.text || (open ? changes.unfinishedText(d.path, layer) : "");
    changes.setEditor(d.path, layer, next && { ...next, text, open }, next === null);
  };

  // Open the comment box for a pointer selection, not when keyboard state paints a range.
  const onLineSelected: FileDiffOptions<Note, undefined>["onLineSelected"] = (range) => {
    const anchor = range ? indexOf(lines, range.side, range.start) : -1;
    const cursor = range ? indexOf(lines, range.endSide ?? range.side, range.end) : -1;
    if (anchor < 0 || cursor < 0) return pick(null);
    pick({ anchor, cursor }, true);
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
  const item = useMemo(() => full ? parsePatchFiles(patch)[0]?.files[0] : null, [full, patch]);
  const [revision, setRevision] = useState({ item, notes, version: 0 });
  if (item && (revision.item !== item || revision.notes !== notes)) {
    setRevision({ item, notes, version: revision.version + 1 });
  }
  const items = useMemo<CodeViewDiffItem<Note>[]>(() => item ? [{ type: "diff", id: ITEM_ID, fileDiff: item, annotations: notes, version: revision.version }] : [], [item, notes, revision.version]);

  if (d.notice) return <p className="px-3 py-3 text-xs text-muted-foreground">{d.notice}</p>;
  if (!d.display || !d.patch) return <pre className="patch cpatch" data-testid="diff-body">{d.patch ?? ""}</pre>;
  if (!lines.length) return <p className="px-3 py-3 text-xs text-muted-foreground">No text changes.</p>;

  const onKey = (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget || e.metaKey || e.ctrlKey || e.altKey) return;
    const edge = e.key === "Home" ? 1 : e.key === "End" ? -1 : 0;
    const dir = edge || { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[e.key];

    if (dir) {
      e.preventDefault();
      let i = sel && !edge ? sel.cursor + dir : dir > 0 ? 0 : lines.length - 1;
      while (i >= 0 && i < lines.length && !code(i)) i += dir;
      if (i < 0 || i >= lines.length) return;
      pick(e.shiftKey && sel ? { anchor: sel.anchor, cursor: i } : { anchor: i, cursor: i });
      const spot = spotOf(lines[i])!;
      if (full) window.current?.scrollTo({ type: "line", id: ITEM_ID, ...spot, align: "nearest", behavior: "instant" });
      else rowAt(e.currentTarget, spot)?.scrollIntoView({ block: "nearest" });

    } else if ((e.key === "Enter" || e.key === "c") && sel) {
      e.preventDefault();
      pick(sel, true);
    } else if (e.key === "Escape" && sel) {
      e.preventDefault();
      setSel(null);
      done(true);
    }
  };

  const done = (keepText = false) => {
    changes.setEditor(d.path, d.layer ?? "branch", null, keepText);
    body.current?.focus();
  };
  const renderNote = ({ metadata: n }: { metadata: Note }) => (
    <>
      {n.comments.map((c) => <CommentCard key={c.id} c={c} onRemove={() => changes.confirmRemoveComment(c.id)} />)}
      {n.box && sel && (
        <CommentBox
          label={`${d.path}`}
          text={editor?.text ?? ""}
          onChange={(text) => changes.setEditor(d.path, d.layer ?? "branch", { ...sel, text, open: true })}
          onAdd={async (text) => {
            const captured = changes.editor(d.path, d.layer ?? "branch");
            // The range route keeps its request cap. Full diffs send only the chosen hunk lines.
            const success = await changes.addComment(lo, hi, text, full ? patchText(lines.slice(lo, hi + 1), d.path) : undefined);
            if (success && changes.editor(d.path, d.layer ?? "branch") === captured) {
              setSel(null);
              done();
            }
            return success;
          }}
          onCancel={() => done()}
          onEscape={() => done(true)}
        />
      )}
    </>
  );

  return (
    <>
      <div ref={body} className="cpatch" style={full ? { overflow: "hidden", paddingBottom: 0 } : undefined} tabIndex={0} aria-label="Diff lines" data-testid="diff-body" onKeyDown={onKey}>
        {full ? <CodeView<Note, undefined>
          ref={window}
          className="h-full overflow-auto"
          items={items}
          options={DIFF_OPTIONS}
          selectedLines={selected ? { id: ITEM_ID, range: selected } : null}
          onSelectedLinesChange={(selection) => onLineSelected(selection?.range ?? null)}
          renderAnnotation={(...[annotation]) => renderNote(annotation)}
          disableWorkerPool
        /> : <PatchDiff<Note, undefined> patch={patch} options={{ ...DIFF_OPTIONS, controlledSelection: true, onLineSelectionEnd: onLineSelected }} selectedLines={selected} lineAnnotations={notes} renderAnnotation={renderNote} disableWorkerPool />}
      </div>
      {(d.is_truncated || (d.hidden_lines ?? 0) > 0) && (
        <div className="border-t border-border px-3 py-1.5 text-xs text-muted-foreground" data-testid="diff-capped">
          Showing the first {plural(lines.length, "line")} of {plural(d.full_lines ?? lines.length, "line")}.
          {expanded.state === "error" && <p role="status" className="text-failed">{expanded.error}</p>}
          {!(expanded.state === "error" && expanded.capped) && (
            <CutLine count={d.hidden_lines ? `${d.hidden_lines.toLocaleString()} more` : "More text"} show={() => void changes.expandDiff()} busy={expanded.state === "loading"} />
          )}
        </div>
      )}
      {!composing && !sel && app.view.openId && <p className="diff-hint">Click a line number to comment; Shift-click picks a range.</p>}
    </>
  );
}

function CommentBox({ label, text, onChange, onAdd, onCancel, onEscape }: {
  label: string; text: string; onChange: (text: string) => void; onAdd: (text: string) => Promise<boolean>;
  onCancel: () => void; onEscape: () => void;
}) {
  const [pending, setPending] = useState(false);
  const submitting = useRef(false);
  const input = useRef<HTMLTextAreaElement>(null);
  const restoreFocus = useRef(false);

  useLayoutEffect(() => {
    if (pending || !restoreFocus.current) return;
    restoreFocus.current = false;
    input.current?.focus();
  }, [pending]);

  const add = async () => {
    if (submitting.current || !text.trim()) return;
    submitting.current = true;
    setPending(true);
    try {
      restoreFocus.current = !(await onAdd(text));
    } finally {
      submitting.current = false;
      setPending(false);
    }
  };

  return (
    <div className="comment-box" data-testid="comment-box">
      <MessageSquarePlusIcon className="mt-1 size-3.5 shrink-0 text-muted-foreground" />
      <textarea
        ref={input}
        autoFocus
        rows={2}
        value={text}
        aria-label={`Comment on ${label}`}
        placeholder="What should the agent do with these lines?"
        data-testid="comment-input"
        disabled={pending}

        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            void add();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onEscape();
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
      <span className="text-xs text-muted-foreground">{c.lines}</span>
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
