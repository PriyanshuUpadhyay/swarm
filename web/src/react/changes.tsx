// The workspace panel beside the chat: Changes (file list, diff, file preview, line comments) and the
// branch's PR with its checks, in jean's look. The diff preview is capped at 200 lines by the server,
// so its lines render plainly. One list per tab; rows are plain elements.

import { FileDiffIcon, GitPullRequestIcon, MessageSquarePlusIcon, RefreshCwIcon, SendIcon, XIcon } from "lucide-react";
import { Fragment, memo, useEffect, useMemo, useRef, useState, useSyncExternalStore, type KeyboardEvent, type MouseEvent } from "react";
import type { App } from "../core/app";
import { checkState, diffLines, fileKey, safeUrl, type ChangeFile, type Changes, type Check, type DiffComment, type FileDiff, type Load, type PrView } from "../core/changes";
import type { PatchLine } from "../core/transcript";
import { Button } from "../components/ui/button";
import { StatusGlyph, type Status } from "./status";

const plural = (n: number, word: string) => `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
const LAYER: Record<string, string> = { staged: "Staged", unstaged: "Unstaged", untracked: "Untracked", conflicted: "Conflict", branch: "" };

/** The two toggles at the end of the tab strip row. */
export function PanelToggles({ changes }: { changes: Changes }) {
  const open = useSyncExternalStore(changes.subscribe, () => changes.view.open);
  const tab = useSyncExternalStore(changes.subscribe, () => changes.view.tab);
  return (
    <div className="panel-toggles">
      <button className="icon-btn" aria-label="Changes" aria-pressed={open && tab === "changes"} title="Changes (⌥⌘I)" data-testid="show-changes" onClick={() => changes.toggle("changes")}>
        <FileDiffIcon />
      </button>
      <button className="icon-btn" aria-label="Pull request" aria-pressed={open && tab === "pr"} title="Pull request (⌥⌘P)" data-testid="show-pr" onClick={() => changes.toggle("pr")}>
        <GitPullRequestIcon />
      </button>
    </div>
  );
}

export function WorkspacePanel({ app, changes }: { app: App; changes: Changes }) {
  const view = useSyncExternalStore(changes.subscribe, () => changes.view);
  if (!view.open) return null;
  return (
    <aside className="wpanel" aria-label="Workspace panel" data-testid="workspace-panel">
      <div className="wpanel-head">
        <div className="seg" role="tablist" aria-label="Panel">
          <button type="button" role="tab" aria-pressed={view.tab === "changes"} aria-selected={view.tab === "changes"} onClick={() => changes.show("changes")}>
            Changes
          </button>
          <button type="button" role="tab" aria-pressed={view.tab === "pr"} aria-selected={view.tab === "pr"} onClick={() => changes.show("pr")}>
            Pull request
          </button>
        </div>
        <span className="flex-1" />
        <button className="icon-btn" aria-label="Refresh" title="Refresh" data-testid="panel-refresh" onClick={() => changes.reload()}>
          <RefreshCwIcon />
        </button>
        <button className="icon-btn" aria-label="Close panel" title="Close (⌥⌘I)" onClick={() => changes.close()}>
          <XIcon />
        </button>
      </div>
      {!view.workspace ? (
        <Note title="No workspace" body="Open a workspace to see its changes." />
      ) : view.tab === "pr" ? (
        <PrPanel pr={view.pr} />
      ) : (
        <ChangesPanel app={app} changes={changes} />
      )}
    </aside>
  );
}

function ChangesPanel({ app, changes }: { app: App; changes: Changes }) {
  const view = useSyncExternalStore(changes.subscribe, () => changes.view);
  const list = view.list;
  const files = list.state === "ready" ? list.data.files : [];
  const refs = list.state === "ready" ? list.data.refs : [];
  const totals = useMemo(() => files.reduce((t, f) => ({ added: t.added + (f.added ?? 0), removed: t.removed + (f.removed ?? 0) }), { added: 0, removed: 0 }), [files]);
  const onKey = (e: KeyboardEvent) => {
    const by = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1, Home: "home", End: "end" }[e.key] as number | "home" | "end" | undefined;
    if (by === undefined || e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    changes.move(by);
  };
  const onClick = (e: MouseEvent) => {
    const key = (e.target as Element).closest<HTMLElement>('[data-testid="change-row"]')?.dataset.key;
    const f = files.find((x) => fileKey(x) === key);
    if (f) void changes.select(f);
  };
  // The selected row stays in view as keys move it.
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (view.selected) listRef.current?.querySelector(`[data-key="${CSS.escape(view.selected)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [view.selected]);

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="changes-panel">
      <div className="wpanel-bar">
        <div className="seg" role="group" aria-label="Compare">
          <button type="button" aria-pressed={view.mode === "local"} data-testid="mode-local" onClick={() => changes.setMode("local")} title="Staged, unstaged, untracked and conflicted files">
            Local
          </button>
          <button type="button" aria-pressed={view.mode === "branch"} data-testid="mode-branch" onClick={() => changes.setMode("branch")} title="Commits on this branch since its base">
            Branch
          </button>
        </div>
        {view.mode === "branch" && (
          <label className="flex min-w-0 items-center gap-1 text-xs text-muted-foreground">
            vs
            <select className="base-select" aria-label="Compare with" data-testid="base-select" value={view.base ?? ""} onChange={(e) => changes.setBase(e.target.value)}>
              {refs.map((r) => (
                <option key={r} value={r}>
                  {r.replace(/^refs\/(heads|remotes)\//, "")}
                </option>
              ))}
            </select>
          </label>
        )}
        <span className="flex-1" />
        {list.state === "ready" && (
          <span className="shrink-0 text-xs text-muted-foreground" data-testid="changes-summary">
            {plural(files.length, "file")} <span className="text-done">+{totals.added.toLocaleString()}</span> <span className="text-failed">−{totals.removed.toLocaleString()}</span>
          </span>
        )}
      </div>
      <div
        ref={listRef}
        className="changes-list"
        role="listbox"
        aria-label="Changed files"
        tabIndex={0}
        data-testid="changes-list"
        data-state={list.state}
        onKeyDown={onKey}
        onClick={onClick}
      >
        {list.state === "loading" && <p className="px-3 py-2 text-xs text-muted-foreground">Reading changes…</p>}
        {list.state === "error" && <p className="px-3 py-2 text-xs break-words text-failed select-text">{list.error}</p>}
        {list.state === "ready" && files.length === 0 && <p className="px-3 py-2 text-xs text-muted-foreground">{view.mode === "local" ? "No local changes." : "No commits since the base."}</p>}
        {files.map((f) => (
          <ChangeRow key={fileKey(f)} f={f} selected={fileKey(f) === view.selected} />
        ))}
      </div>
      <DiffPane app={app} changes={changes} />
      <Drafts app={app} changes={changes} files={files} />
    </div>
  );
}

const STATUS_CLASS: Record<string, string> = { A: "add", "?": "add", D: "del", U: "del" };

const ChangeRow = memo(function ChangeRow({ f, selected }: { f: ChangeFile; selected: boolean }) {
  const slash = f.path.lastIndexOf("/");
  const status = f.layer === "conflicted" ? "U" : f.status.slice(0, 1);
  return (
    <div role="option" aria-selected={selected} data-testid="change-row" data-key={fileKey(f)} data-path={f.path} data-layer={f.layer} className="change-row" title={f.path}>
      <span className={`st ${STATUS_CLASS[status] ?? "mod"}`} aria-label={`Status ${f.status}`}>
        {status}
      </span>
      <span className="min-w-0 flex-1 truncate">
        <span className="text-foreground">{f.path.slice(slash + 1)}</span>
        {slash > 0 && <span className="ml-1.5 text-faint-foreground">{f.path.slice(0, slash)}</span>}
      </span>
      {LAYER[f.layer] && <span className="layer">{LAYER[f.layer]}</span>}
      <span className="counts num">
        {f.added === null ? "bin" : <span className="text-done">+{f.added}</span>} {f.removed !== null && <span className="text-failed">−{f.removed}</span>}
      </span>
    </div>
  );
});

function DiffPane({ app, changes }: { app: App; changes: Changes }) {
  const view = useSyncExternalStore(changes.subscribe, () => changes.view);
  const d = view.diff;
  if (d.state === "idle") return <div className="diff-pane" />;
  const data = d.state === "ready" ? d.data : null;
  return (
    <div className="diff-pane" data-testid="diff-pane" data-state={d.state}>
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
  const [sel, setSel] = useState<{ anchor: number; cursor: number } | null>(null);
  const [composing, setComposing] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  // A new file or a new patch drops the selection.
  const [shown, setShown] = useState(d.patch);
  if (shown !== d.patch) {
    setShown(d.patch);
    setSel(null);
    setComposing(false);
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

function CommentBox({ label, onAdd, onCancel }: { label: string; onAdd: (text: string) => void; onCancel: () => void }) {
  const [text, setText] = useState("");
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
        onChange={(e) => setText(e.target.value)}
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

/** Every draft of the workspace, across files, and the one Send. */
function Drafts({ app, changes, files }: { app: App; changes: Changes; files: ChangeFile[] }) {
  const comments = useSyncExternalStore(changes.subscribe, () => changes.view.comments);
  const sending = useSyncExternalStore(changes.subscribe, () => changes.view.sending);
  // The open chat and its chair's state decide whether a send can go now.
  const blocked = useSyncExternalStore(app.subscribe, () => changes.sendBlocked());
  if (!comments.length) return null;
  return (
    <div className="drafts" data-testid="drafts">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium">{plural(comments.length, "comment")}</span>
        <span className="flex-1" />
        <Button size="sm" data-testid="send-comments" disabled={sending || blocked !== null} title={blocked ?? "Send every comment to the chat's agent as one message"} onClick={() => void changes.sendComments()}>
          <SendIcon /> {sending ? "Sending…" : "Send to agent"}
        </Button>
      </div>
      <ul className="mt-1 grid max-h-28 gap-0.5 overflow-y-auto">
        {comments.map((c) => (
          <li key={c.id} className="draft-row">
            <button
              type="button"
              className="min-w-0 flex-1 truncate text-left"
              title={c.text}
              onClick={() => {
                const f = files.find((x) => x.path === c.path && x.layer === c.layer);
                if (f) void changes.select(f);
              }}
            >
              <span className="text-foreground">{c.path.slice(c.path.lastIndexOf("/") + 1)}</span> <span className="text-faint-foreground">{c.lines}</span> {c.text}
            </button>
            <button className="icon-btn" aria-label="Remove comment" onClick={() => changes.removeComment(c.id)}>
              <XIcon />
            </button>
          </li>
        ))}
      </ul>
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

// ---- PR ----

const CHECK_STATUS: Record<ReturnType<typeof checkState>, Status> = { failed: "failed", pending: "working", passed: "done", skipped: "ended" };
const CHECK_ORDER = ["failed", "pending", "passed", "skipped"];
const checkName = (c: Check) => [c.workflowName, c.name ?? c.context].filter(Boolean).join(" / ") || "Check";

function PrPanel({ pr }: { pr: Load<PrView> }) {
  if (pr.state === "idle" || pr.state === "loading")
    return (
      <div className="wpanel-body" tabIndex={-1} data-testid="pr-panel" data-state="loading">
        <p className="text-xs text-muted-foreground">Asking GitHub for this branch's pull request…</p>
      </div>
    );
  if (pr.state === "error")
    return (
      <div className="wpanel-body" tabIndex={-1} data-testid="pr-panel" data-state="error">
        <p className="text-xs break-words text-failed select-text">{pr.error}</p>
      </div>
    );
  const p = pr.data.pullRequest;
  if (!p)
    return (
      <div className="wpanel-body" tabIndex={-1} data-testid="pr-panel" data-state="none">
        <Note title="No open pull request" body="GitHub has no open pull request for this branch." />
      </div>
    );
  const checks = [...(pr.data.checks ?? [])].sort((a, b) => CHECK_ORDER.indexOf(checkState(a)) - CHECK_ORDER.indexOf(checkState(b)));
  const count = (s: string) => checks.filter((c) => checkState(c) === s).length;
  const state = p.isDraft ? "Draft" : p.state[0] + p.state.slice(1).toLowerCase();
  const url = safeUrl(p.url);
  return (
    <div className="wpanel-body" tabIndex={-1} data-testid="pr-panel" data-state="ready">
      <div className="grid gap-1.5">
        <div className="flex items-start gap-2">
          <span className={`pr-state ${p.isDraft ? "draft" : p.state.toLowerCase()}`} data-testid="pr-state">
            {state}
          </span>
          <h2 className="min-w-0 flex-1 text-[13px] font-semibold break-words" data-testid="pr-title">
            {p.title}{" "}
            {url ? (
              <a className="font-normal text-muted-foreground hover:text-foreground" href={url} target="_blank" rel="noreferrer">
                #{p.number}
              </a>
            ) : (
              <span className="font-normal text-muted-foreground">#{p.number}</span>
            )}
          </h2>
        </div>
        <p className="font-mono text-xs text-muted-foreground">
          {p.baseRefName} ← {p.headRefName}
        </p>
        {p.reviewDecision && <p className="text-xs text-muted-foreground">Review: {p.reviewDecision.replaceAll("_", " ").toLowerCase()}</p>}
        {pr.data.differsFromLocalHead && <p className="text-xs text-waiting">The PR head is not your local HEAD. Push or pull to match them.</p>}
      </div>
      <div className="mt-4 flex items-center gap-2 text-xs">
        <span className="font-medium">Checks</span>
        <span className="text-muted-foreground" data-testid="checks-summary">
          {checks.length === 0 ? "none" : [count("failed") && `${count("failed")} failed`, count("pending") && `${count("pending")} pending`, count("passed") && `${count("passed")} passed`, count("skipped") && `${count("skipped")} skipped`].filter(Boolean).join(", ")}
        </span>
      </div>
      <ul className="mt-1 grid gap-0.5" data-testid="checks">
        {checks.map((c, i) => {
          const s = checkState(c);
          const link = safeUrl(c.detailsUrl ?? c.targetUrl);
          return (
            <li key={i} className="check-row" data-testid="check" data-state={s}>
              <StatusGlyph status={CHECK_STATUS[s]} />
              <span className="min-w-0 flex-1 truncate" title={checkName(c)}>
                {checkName(c)}
              </span>
              <span className="text-muted-foreground">{s}</span>
              {link && (
                <a className="text-muted-foreground hover:text-foreground" href={link} target="_blank" rel="noreferrer">
                  Details
                </a>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function Note({ title, body }: { title: string; body: string }) {
  return (
    <div role="status" className="mx-3 mt-4 rounded-lg border border-dashed border-border-strong px-3 py-4 text-center">
      <div className="text-sm font-medium text-foreground">{title}</div>
      <p className="mt-1 text-xs text-muted-foreground">{body}</p>
    </div>
  );
}
