// New workspace creation, issue selection, and setup progress.

import { LoaderCircleIcon } from "lucide-react";
import { useEffect, useState } from "react";
import type { App } from "../core/app";
import { setupFailure, type Issue, type Mode, type NewOptions, type NewRequest } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { FIELD, message, useLife, Frame, Label, ErrorLine } from "./lifecycle-shared";

// ---- New Workspace ----

const MODES: [Mode, string][] = [
  ["new-branch", "New branch"],
  ["existing-branch", "Existing branch"],
  ["pr", "Pull request"],
  ["issue", "Issue"],
];

function NewWorkspaceDialog({ app }: { app: App }) {
  const nw = useLife(app, "newWorkspace");
  return (
    <Frame open={nw !== null} onClose={() => app.life.close("newWorkspace")} title="New workspace" testid="new-workspace" wide>
      {nw && <NewWorkspaceBody app={app} initialProject={nw.project} key={nw.project} />}
    </Frame>
  );
}

function NewWorkspaceBody({ app, initialProject }: { app: App; initialProject: string }) {
  const create = useLife(app, "create");
  const projects = app.life.projects();
  const [project, setProject] = useState(initialProject);
  const [mode, setMode] = useState<Mode>("new-branch");
  const [opts, setOpts] = useState<{ project: string; options: NewOptions | null; error: string | null } | null>(null);
  const [name, setName] = useState("");
  const [base, setBase] = useState("");
  const [branch, setBranch] = useState("");
  const [pr, setPr] = useState("");
  const [issue, setIssue] = useState<Issue | null>(null);
  const [folder, setFolder] = useState("");
  const [prefix, setPrefix] = useState("");
  const working = create?.state === "working";
  const kept = create?.state === "kept";

  useEffect(() => {
    let live = true;
    setOpts(null);
    app.life.options(project).then(
      (options) => {
        if (!live) return;
        setOpts({ project, options, error: null });
        setBase(options.bases[0] ?? "");
        setBranch(options.branches[0] ?? "");
        setFolder(options.folder);
        setPrefix(options.prefix);
      },
      (err) => live && setOpts({ project, options: null, error: message(err) }),
    );
    return () => {
      live = false;
    };
  }, [app, project]);

  const options = opts?.project === project ? opts.options : null;
  const unborn = options !== null && options.defaultBranch === null;
  const request = (): NewRequest | null => {
    const common = { project, folder, prefix };
    if (mode === "issue") return issue ? { ...common, mode, name: "", issue: issue.number, base: base || undefined } : null;
    if (!name.trim()) return null;
    if (mode === "new-branch") return { ...common, mode, name: name.trim(), base: unborn ? undefined : base };
    if (mode === "existing-branch") return branch ? { ...common, mode, name: name.trim(), branch } : null;
    const n = Number(pr);
    return Number.isInteger(n) && n > 0 ? { ...common, mode, name: name.trim(), pr: n } : null;
  };
  const req = request();
  const submit = () => req && !working && void app.life.create(req);

  return (
    <form
      className="grid gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <fieldset disabled={working || kept} className="grid gap-3">
        <Label text="Project">
          <select className={FIELD} data-testid="nw-project" value={project} onChange={(e) => setProject(e.target.value)}>
            {projects.map((p) => (
              <option key={p.path} value={p.path}>
                {p.title} — {p.path}
              </option>
            ))}
          </select>
        </Label>
        <div role="tablist" aria-label="Start from" className="view-switch !mx-0 !mb-0">
          {MODES.map(([m, label]) => (
            <button key={m} type="button" role="tab" aria-selected={mode === m} data-testid={`nw-mode-${m}`} disabled={unborn && m !== "new-branch"} onClick={() => setMode(m)}>
              {label}
            </button>
          ))}
        </div>
        {opts?.error && <ErrorLine text={opts.error} />}
        {!opts && <p className="text-sm text-muted-foreground">Reading branches…</p>}
        {options && (
          <>
            {mode !== "issue" && (
              <Label text="Name">
                <input autoFocus className={FIELD} data-testid="nw-name" value={name} onChange={(e) => setName(e.target.value)} placeholder={mode === "pr" ? "Review 42" : "Fix sidebar"} />
              </Label>
            )}
            {(mode === "new-branch" || mode === "issue") &&
              (unborn ? (
                <p className="text-sm text-muted-foreground">This repository has no commits yet, so the first workspace starts a new branch with no base.</p>
              ) : (
                <Label text="Base branch">
                  <select className={FIELD} data-testid="nw-base" value={base} onChange={(e) => setBase(e.target.value)}>
                    {options.bases.map((b) => (
                      <option key={b}>{b}</option>
                    ))}
                  </select>
                </Label>
              ))}
            {mode === "existing-branch" &&
              (options.branches.length ? (
                <Label text="Branch">
                  <select
                    className={FIELD}
                    data-testid="nw-branch"
                    value={branch}
                    onChange={(e) => {
                      setBranch(e.target.value);
                      if (!name) setName(e.target.value.split("/").pop() ?? "");
                    }}
                  >
                    {options.branches.map((b) => (
                      <option key={b}>{b}</option>
                    ))}
                  </select>
                </Label>
              ) : (
                <p className="text-sm text-muted-foreground">Every branch is already checked out in a workspace.</p>
              ))}
            {mode === "pr" && (
              <Label text="Pull request number">
                <input className={FIELD} data-testid="nw-pr" inputMode="numeric" value={pr} onChange={(e) => setPr(e.target.value.replace(/\D/g, ""))} placeholder="42" />
              </Label>
            )}
            {mode === "issue" && <IssuePicker app={app} project={project} selected={issue} onSelect={setIssue} />}
            <div className="grid grid-cols-[2fr_1fr] gap-2">
              <Label text="Worktree folder">
                <input className={FIELD} data-testid="nw-folder" value={folder} onChange={(e) => setFolder(e.target.value)} />
              </Label>
              <Label text="Branch prefix">
                <input className={FIELD} data-testid="nw-prefix" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
              </Label>
            </div>
            {options.setup_script && (
              <p className="text-xs text-muted-foreground">
                The setup script runs in the new workspace: <code className="font-mono">{options.setup_script.split("\n")[0]}</code>
              </p>
            )}
          </>
        )}
      </fieldset>
      <CreateProgress app={app} />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("newWorkspace")}>
          {kept ? "Close" : "Cancel"}
        </Button>
        {kept ? (
          <Button type="button" size="sm" data-testid="nw-open" onClick={() => void app.life.openKept()}>
            Open workspace
          </Button>
        ) : (
          <Button type="submit" size="sm" data-testid="nw-create" disabled={!req || working}>
            {working && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
            Create
          </Button>
        )}
      </div>
    </form>
  );
}

/** Open issues from the project's GitHub origin; the picked one names the branch and the first prompt. */
function IssuePicker({ app, project, selected, onSelect }: { app: App; project: string; selected: Issue | null; onSelect: (issue: Issue) => void }) {
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
          <div role="listbox" aria-label="Open issues" data-testid="nw-issues" className="max-h-[180px] overflow-y-auto rounded-md border border-border">
            {shown.length === 0 && <p className="px-2 py-2 text-sm text-muted-foreground">{list.issues.length ? "No issue matches." : "This repository has no open issues."}</p>}
            {shown.map((i) => (
              <button
                key={i.number}
                type="button"
                role="option"
                aria-selected={selected?.number === i.number}
                data-testid="nw-issue"
                className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-hover aria-selected:bg-selected"
                onClick={() => onSelect(i)}
              >
                <span className="shrink-0 text-muted-foreground tabular-nums">#{i.number}</span>
                <span className="min-w-0 flex-1 truncate">{i.title}</span>
              </button>
            ))}
          </div>
          {selected && (
            <p className="line-clamp-3 text-xs break-words whitespace-pre-wrap text-muted-foreground" data-testid="nw-issue-body">
              The new chat starts with this issue in its message box. {selected.body.trim() ? selected.body.trim() : "(The issue has no body.)"}
            </p>
          )}
        </>
      )}
    </div>
  );
}

/** Creating, the live setup output, and a failure. */
function CreateProgress({ app }: { app: App }) {
  const create = useLife(app, "create");
  if (!create) return null;
  const setup = create.setup;
  return (
    <div className="grid gap-2" data-testid="nw-progress" data-state={create.state} data-setup={setup?.state}>
      {create.state === "working" && (
        <p className="flex items-center gap-1.5 text-sm text-muted-foreground" role="status">
          <LoaderCircleIcon className="size-3.5 animate-spin motion-reduce:animate-none" />
          {setup ? "Running the setup script…" : "Creating the workspace…"}
        </p>
      )}
      <ErrorLine text={create.error} />
      {setup && (
        <pre data-testid="setup-output" aria-label="Setup output" className="max-h-[220px] overflow-auto rounded-md border border-border bg-code p-2 font-mono text-xs break-words whitespace-pre-wrap select-text">
          {setup.output || (setup.state === "running" ? "Waiting for output…" : "(no output)")}
        </pre>
      )}
      {create.state === "kept" && setup && <ErrorLine text={setupFailure(setup)} />}
    </div>
  );
}

export { NewWorkspaceDialog };
