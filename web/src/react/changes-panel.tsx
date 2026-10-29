// The workspace panel beside the chat: Changes (file list, diff, file preview, line comments) and the
// branch's PR with its checks, in jean's look. The diff preview is capped at 200 lines by the server,
// so its lines render plainly. One list per tab; rows are plain elements.

import { FileDiffIcon, GitPullRequestIcon, RefreshCwIcon, SendIcon, XIcon } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useSyncExternalStore, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { App } from "../core/app";
import { checkState, fileKey, safeUrl, type ChangeFile, type Changes, type Check, type Load, type PrView } from "../core/changes";
import { Button } from "../components/ui/button";
import { StatusGlyph, type Status } from "./status";
import { DiffPane } from "./changes-diff";
import { plural } from "./changes-shared";

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
  useEffect(() => {
    if (!view.open || !view.workspace) return;
    const refresh = () => { if (!document.hidden && changes.view.list.state !== "loading" && changes.view.pr.state !== "loading") changes.reload(); };
    const timer = window.setInterval(refresh, 5000);
    window.addEventListener("focus", refresh);
    return () => { clearInterval(timer); window.removeEventListener("focus", refresh); };
  }, [changes, view.open, view.workspace]);
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
  const [query, setQuery] = useState("");
  const list = view.list;
  const files = list.state === "ready" ? list.data.files : [];
  const shown = files.filter((f) => f.path.toLowerCase().includes(query.trim().toLowerCase()));
  const refs = list.state === "ready" ? list.data.refs : [];
  const totals = useMemo(() => files.reduce((t, f) => ({ added: t.added + (f.added ?? 0), removed: t.removed + (f.removed ?? 0) }), { added: 0, removed: 0 }), [files]);
  const onKey = (e: KeyboardEvent) => {
    const by = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1, Home: "home", End: "end" }[e.key] as number | "home" | "end" | undefined;
    if (by === undefined || e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    changes.move(by, query);
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
      <input aria-label="Filter changed files" placeholder="Filter by path" value={query} onChange={(e) => setQuery(e.target.value)} className="mx-3 my-1 rounded border px-2 py-1 text-xs" />
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
        {files.length > 0 && !shown.length && <p className="px-3 py-2 text-xs text-muted-foreground">No paths match.</p>}
        {shown.map((f) => (
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
