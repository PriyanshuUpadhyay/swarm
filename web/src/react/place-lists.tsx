// The place menu of the new chat composer (ADR 0147): a new worktree, an existing branch, a pull
// request, an issue, or the folder itself. Its lists replace the old new workspace dialog.

import { LoaderCircleIcon } from "lucide-react";
import { useEffect, useState, type KeyboardEvent } from "react";
import type { App } from "../core/app";
import type { Issue, NewOptions } from "../core/lifecycle";
import type { Place, Target } from "../core/new-chat";
import { Button } from "../components/ui/button";
import { FIELD, message, ErrorLine } from "./lifecycle-shared";

type Kind = Place["kind"];

/** Each kind's tab word and its place before a pick; the type needs every kind. */
const KINDS: { [K in Kind]: [string, Extract<Place, { kind: K }>] } = {
  new: ["New worktree", { kind: "new" }],
  branch: ["Existing branch", { kind: "branch", branch: "" }],
  pr: ["Pull request", { kind: "pr", number: 0 }],
  issue: ["Issue", { kind: "issue", number: 0 }],
  here: ["This folder", { kind: "here" }],
};

/** The menu body: the kinds, then the list of the picked kind. */
export function PlacePanel({ app, target, place, onPlace }: { app: App; target: Target; place: Place; onPlace: (place: Place) => void }) {
  const project = target.project;
  const [opts, setOpts] = useState<{ options: NewOptions | null; error: string | null } | null>(null);
  const [reads, setReads] = useState(0);
  const [fetching, setFetching] = useState<{ error: string | null } | null>(null);

  useEffect(() => {
    let live = true;
    setOpts(null);
    app.life.options(project).then(
      (options) => live && setOpts({ options, error: null }),
      (err) => live && setOpts({ options: null, error: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [app, project, reads]);

  const fetchOrigin = () => {
    setFetching({ error: null });
    app.life.fetchOrigin(project).then(
      () => {
        setFetching(null);
        setReads((n) => n + 1);
      },
      (err) => setFetching({ error: message(err) }),
    );
  };
  const options = opts?.options ?? null;
  const unborn = options !== null && options.defaultBranch === null;
  const pick = (kind: Kind) => onPlace(KINDS[kind][1]);
  const kinds = Object.entries(KINDS) as [Kind, [string, Place]][];

  return (
    <div className="grid gap-3" data-testid="place-panel">
      <div role="tablist" aria-label="Start in" className="view-switch !mx-0 !mb-0">
        {kinds.map(([k, [label]]) => (
          <button key={k} type="button" role="tab" aria-selected={place.kind === k} data-testid={`place-kind-${k}`} disabled={unborn && k !== "new" && k !== "here"} onClick={() => pick(k)}>
            {label}
          </button>
        ))}
      </div>
      {place.kind === "new" && <p className="text-sm text-muted-foreground">The chat gets its own worktree, on a branch named from your prompt.</p>}
      {place.kind === "here" && <p className="text-sm text-muted-foreground">The chat runs in <code className="font-mono break-all">{target.cwd}</code>, with no new worktree.</p>}
      {(place.kind === "branch" || place.kind === "pr") && options && !unborn && (
        <div className="flex items-center justify-end gap-2">
          {fetching?.error && <ErrorLine text={fetching.error} />}
          <Button type="button" variant="ghost" size="sm" data-testid="place-fetch" disabled={fetching !== null && !fetching.error} onClick={fetchOrigin}>
            {fetching && !fetching.error && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
            Fetch origin
          </Button>
        </div>
      )}
      {place.kind === "branch" && opts?.error && <ErrorLine text={opts.error} />}
      {place.kind === "branch" && !opts && <p className="text-sm text-muted-foreground">Reading branches…</p>}
      {place.kind === "branch" &&
        options &&
        (branchChoices(options).length ? (
          <BranchPicker label="Branch" testid="place-branch" branches={branchChoices(options)} open={options.checkouts} value={place.branch} onChange={(branch) => onPlace({ kind: "branch", branch })} />
        ) : (
          <p className="text-sm text-muted-foreground">This project has no branch but the default one.</p>
        ))}
      {place.kind === "pr" && (
        <label className="grid gap-1 text-sm text-muted-foreground">
          Pull request number
          <input
            className={FIELD}
            data-testid="place-pr"
            inputMode="numeric"
            value={place.number ? String(place.number) : ""}
            placeholder="42"
            onChange={(e) => onPlace({ kind: "pr", number: Number(e.target.value.replace(/\D/g, "")) })}
          />
        </label>
      )}
      {place.kind === "issue" && <IssuePicker app={app} project={project} selected={place.number} onSelect={(i) => onPlace({ kind: "issue", number: i.number })} />}
      {options?.setup_script && place.kind !== "here" && (
        <p className="text-xs text-muted-foreground">
          The setup script runs in the new worktree: <code className="font-mono">{options.setup_script.split("\n")[0]}</code>
        </p>
      )}
    </div>
  );
}

/** The Existing branch list: the open worktrees first, then the free branches. */
function branchChoices(options: NewOptions): string[] {
  return [...Object.keys(options.checkouts ?? {}), ...options.branches];
}

/** A filter box over a branch list, so a long list needs no scrolling. Arrow keys move the pick.
 *  A branch in `open` is checked out already, so picking it opens that worktree. */
function BranchPicker({ label, testid, branches, open, value, onChange }: { label: string; testid: string; branches: string[]; open?: Record<string, string>; value: string; onChange: (branch: string) => void }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const shown = q ? branches.filter((b) => b.toLowerCase().includes(q)) : branches;
  const onKeyDown = (e: KeyboardEvent) => {
    const at = shown.indexOf(value);
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step && shown.length) onChange(shown[Math.min(shown.length - 1, Math.max(0, at + step))]);
    // Enter confirms the pick in the list, or the first match, and does not submit the form.
    else if (e.key === "Enter" && shown.length) onChange(at >= 0 ? value : shown[0]);
    else return;
    e.preventDefault();
    // ⌘Enter here only picks: the send would still carry the place from before the pick.
    e.stopPropagation();
  };
  return (
    <div className="grid gap-1">
      <span className="text-sm text-muted-foreground">
        {label}: <span className="font-mono text-foreground">{value || "none"}</span>
      </span>
      <input className={FIELD} aria-label={`Filter ${label.toLowerCase()}`} data-testid={`${testid}-filter`} placeholder={`Filter ${branches.length} branches`} value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={onKeyDown} />
      <div role="listbox" aria-label={label} data-testid={testid} className="max-h-[160px] overflow-y-auto rounded-md border border-border">
        {shown.length === 0 && <p className="px-2 py-2 text-sm text-muted-foreground">No branch matches.</p>}
        {shown.map((b) => (
          <button
            key={b}
            type="button"
            role="option"
            aria-selected={b === value}
            ref={b === value ? (el) => el?.scrollIntoView({ block: "nearest" }) : undefined}
            className="flex w-full gap-2 px-2 py-1 text-left font-mono text-sm hover:bg-hover aria-selected:bg-selected"
            onClick={() => onChange(b)}
          >
            <span className="min-w-0 flex-1 truncate">{b}</span>
            {open?.[b] && (
              <span className="shrink-0 text-xs text-muted-foreground" title={open[b]}>
                open
              </span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Open issues from the project's GitHub origin; the picked one names the branch, and its text goes before the prompt. */
function IssuePicker({ app, project, selected, onSelect }: { app: App; project: string; selected: number; onSelect: (issue: Issue) => void }) {
  const [list, setList] = useState<{ issues: Issue[] | null; error: string | null }>({ issues: null, error: null });
  const [query, setQuery] = useState("");
  useEffect(() => {
    let live = true;
    setList({ issues: null, error: null });
    app.life.issues(project).then(
      (issues) => live && setList({ issues, error: null }),
      (err) => live && setList({ issues: null, error: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [app, project]);
  const q = query.trim().toLowerCase();
  const shown = (list.issues ?? []).filter((i) => !q || `#${i.number} ${i.title}`.toLowerCase().includes(q));
  return (
    <div className="grid gap-2">
      {list.error && <ErrorLine text={list.error} />}
      {!list.issues && !list.error && <p className="text-sm text-muted-foreground">Reading open issues…</p>}
      {list.issues && (
        <>
          <input className={FIELD} aria-label="Filter issues" placeholder="Filter issues" value={query} onChange={(e) => setQuery(e.target.value)} />
          <div role="listbox" aria-label="Open issues" data-testid="place-issues" className="max-h-[180px] overflow-y-auto rounded-md border border-border">
            {shown.length === 0 && <p className="px-2 py-2 text-sm text-muted-foreground">{list.issues.length ? "No issue matches." : "This repository has no open issues."}</p>}
            {shown.map((i) => (
              <button
                key={i.number}
                type="button"
                role="option"
                aria-selected={selected === i.number}
                data-testid="place-issue"
                className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-hover aria-selected:bg-selected"
                onClick={() => onSelect(i)}
              >
                <span className="shrink-0 text-muted-foreground tabular-nums">#{i.number}</span>
                <span className="min-w-0 flex-1 truncate">{i.title}</span>
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
