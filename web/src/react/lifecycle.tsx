// The lifecycle dialogs (S3): New Workspace with its setup output, Create and Import Project, the
// project setup script, Switch model with its progress, one confirm dialog, and Recently closed.
// Each reads only its own field of `app.life.state`.

import { CheckIcon, CircleIcon, LoaderCircleIcon, TriangleAlertIcon, XIcon } from "lucide-react";
import { useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import type { App } from "../core/app";
import { setupFailure, type ClosedChat, type Issue, type LifeState, type Mode, type NewOptions, type NewRequest } from "../core/lifecycle";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../components/ui/dialog";

const FIELD = "h-8 w-full min-w-0 rounded-md border border-border-strong bg-background px-2 outline-none focus:border-ring disabled:opacity-60";
const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

function useLife<K extends keyof LifeState>(app: App, key: K): LifeState[K] {
  return useSyncExternalStore(app.life.subscribe, () => app.life.state[key]);
}

export function LifecycleDialogs({ app }: { app: App }) {
  return (
    <>
      <NewWorkspaceDialog app={app} />
      <ProjectDialog app={app} />
      <SetupScriptDialog app={app} />
      <SwitchModelDialog app={app} />
      <ConfirmDialog app={app} />
      <ClosedDialog app={app} />
    </>
  );
}

/** One dialog frame: a title, an optional line under it, the body, and a footer. */
function Frame({ open, onClose, title, description, children, testid, wide }: { open: boolean; onClose: () => void; title: string; description?: string; children: ReactNode; testid: string; wide?: boolean }) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      {open && (
        <DialogContent
          className={`${wide ? "w-[min(560px,calc(100vw-32px))]" : "w-[min(440px,calc(100vw-32px))]"} grid max-h-[calc(100vh-104px)] gap-3 overflow-y-auto p-4`}
          data-testid={testid}
          {...(description ? {} : { "aria-describedby": undefined })}
        >
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
          {children}
        </DialogContent>
      )}
    </Dialog>
  );
}

declare global {
  interface Window {
    /** The Mac shell's one script message (ADR 0077); a plain browser has none. */
    webkit?: { messageHandlers?: { swarm?: { postMessage: (body: { kind: "pickFolder" }) => Promise<string | null> } } };
  }
}

function Label({ text, children }: { text: string; children: ReactNode }) {
  return (
    <label className="grid gap-1 text-xs text-muted-foreground">
      {text}
      {children}
    </label>
  );
}

function ErrorLine({ text }: { text: string | null }) {
  if (!text) return null;
  return (
    <p role="alert" data-testid="dialog-error" className="flex items-start gap-1.5 text-sm break-words whitespace-pre-wrap text-failed select-text">
      <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" />
      {text}
    </p>
  );
}

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
            {working && <LoaderCircleIcon className="animate-spin" />}
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
          <LoaderCircleIcon className="size-3.5 animate-spin" />
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

// ---- projects ----

function ProjectDialog({ app }: { app: App }) {
  const mode = useLife(app, "project");
  return (
    <Frame
      open={mode !== null}
      onClose={() => app.life.close("project")}
      title={mode === "create" ? "Create project" : "Import project"}
      description={mode === "create" ? "Swarm makes the folder, runs git init, and makes a first commit." : "Add a folder that is on this Mac."}
      testid="project-dialog"
    >
      {mode && <ProjectBody app={app} mode={mode} key={mode} />}
    </Frame>
  );
}

function ProjectBody({ app, mode }: { app: App; mode: "create" | "import" }) {
  const choice = useLife(app, "importChoice");
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const absolute = path.trim().startsWith("/") || path.trim().startsWith("~/");
  const picker = window.webkit?.messageHandlers?.swarm;
  const save = async (as?: "git" | "folder") => {
    setBusy(true);
    setError(await app.life.saveProject(mode, path.trim(), name.trim() || path.trim().split("/").filter(Boolean).pop() || "", as));
    setBusy(false);
  };
  return (
    <form
      className="grid gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (absolute && !busy) void save();
      }}
    >
      <div className="flex items-end gap-2">
        <div className="grid flex-1">
          <Label text={mode === "create" ? "New folder (full path)" : "Folder (full path)"}>
            <input autoFocus className={`${FIELD} font-mono`} data-testid="project-path" value={path} disabled={busy || choice !== null} onChange={(e) => setPath(e.target.value)} placeholder="/Users/you/code/project" />
          </Label>
        </div>
        {mode === "import" && picker && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid="project-pick"
            disabled={busy || choice !== null}
            onClick={async () => {
              const picked = await picker.postMessage({ kind: "pickFolder" });
              if (picked) setPath(picked);
            }}
          >
            Choose…
          </Button>
        )}
      </div>
      {mode === "create" && (
        <Label text="Name">
          <input className={FIELD} data-testid="project-name" value={name} disabled={busy} onChange={(e) => setName(e.target.value)} placeholder={path.split("/").filter(Boolean).pop() ?? ""} />
        </Label>
      )}
      <ErrorLine text={error} />
      {choice !== null ? (
        <div className="grid gap-2" data-testid="import-choice">
          <p className="text-sm">This folder is not a Git repository. Each workspace in a project is a Git worktree, so a project needs Git.</p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={busy} onClick={() => void save("folder")}>
              Use as a plain folder
            </Button>
            <Button type="button" size="sm" disabled={busy} onClick={() => void save("git")}>
              Run git init
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("project")}>
            Cancel
          </Button>
          <Button type="submit" size="sm" data-testid="project-save" disabled={!absolute || busy}>
            {mode === "create" ? "Create" : "Import"}
          </Button>
        </div>
      )}
    </form>
  );
}

function SetupScriptDialog({ app }: { app: App }) {
  const target = useLife(app, "setupScript");
  return (
    <Frame
      open={target !== null}
      onClose={() => app.life.close("setupScript")}
      title={`Setup script for ${target?.title ?? ""}`}
      description="Runs with /bin/sh in each new workspace of this project. A failure keeps the workspace."
      testid="setup-script-dialog"
    >
      {target && <SetupScriptBody app={app} project={target.project} key={target.project} />}
    </Frame>
  );
}

function SetupScriptBody({ app, project }: { app: App; project: string }) {
  const [script, setScript] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    app.life.options(project).then(
      (o) => live && setScript(o.setup_script ?? ""),
      (err) => {
        if (!live) return;
        setScript("");
        setError(message(err));
      },
    );
    return () => {
      live = false;
    };
  }, [app, project]);
  return (
    <form
      className="grid gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (script !== null) void app.life.saveSetupScript(project, script);
      }}
    >
      <textarea
        autoFocus
        aria-label="Setup script"
        data-testid="setup-script"
        rows={5}
        disabled={script === null}
        className="w-full resize-y rounded-md border border-border-strong bg-background p-2 font-mono text-sm outline-none focus:border-ring"
        placeholder="pnpm install"
        value={script ?? ""}
        onChange={(e) => setScript(e.target.value)}
      />
      <ErrorLine text={error} />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("setupScript")}>
          Cancel
        </Button>
        <Button type="submit" size="sm" data-testid="setup-script-save" disabled={script === null}>
          Save
        </Button>
      </div>
    </form>
  );
}

// ---- Switch model ----

const PHASES: [string, string][] = [
  ["preparing", "Read this chat"],
  ["summarizing", "Ask the agent for a summary"],
  ["starting", "Start the new chat"],
  ["waiting", "Wait for the new agent"],
  ["delivering", "Send the summary"],
];

function SwitchModelDialog({ app }: { app: App }) {
  const old = useLife(app, "switchModel");
  return (
    <Frame open={old !== null} onClose={() => app.life.close("switchModel")} title="Switch model" description="A new chat on the chosen model continues this one with its summary." testid="switch-model">
      {old && <SwitchModelBody app={app} old={old} key={old} />}
    </Frame>
  );
}

function SwitchModelBody({ app, old }: { app: App; old: string }) {
  const handoff = useLife(app, "handoff");
  const [providers, setProviders] = useState<{ id: string; label: string; accounts: boolean }[]>([]);
  const [provider, setProvider] = useState("claude");
  const [models, setModels] = useState<{ id: string; label: string }[] | null>(null);
  const [accounts, setAccounts] = useState<{ name: string; email: string | null }[]>([]);
  const [account, setAccount] = useState("");
  const [query, setQuery] = useState("");
  const [model, setModel] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = handoff !== null && !["completed", "failed", "cancelled"].includes(handoff.phase);

  useEffect(() => {
    app.life.providers().then(
      (p) => setProviders(p.providers.filter((x) => x.id === "claude" || x.id === "codex")),
      (err) => setError(`Could not read the providers: ${message(err)}`),
    );
  }, [app]);
  const hasAccounts = providers.find((p) => p.id === provider)?.accounts ?? false;
  useEffect(() => {
    let live = true;
    setModels(null);
    setModel(null);
    setAccounts([]);
    setAccount("");
    setError(null);
    app.life.models(provider).then(
      (m) => live && setModels(m.models),
      (err) => {
        if (!live) return;
        // A typed model id still works without the list.
        setModels([]);
        setError(`Could not read the models: ${message(err)}`);
      },
    );
    if (hasAccounts)
      app.life.accounts(provider).then(
        (a) => live && setAccounts(a.accounts),
        () => {}, // Accounts are optional; the default account starts.
      );
    return () => {
      live = false;
    };
  }, [app, provider, hasAccounts]);

  const q = query.trim();
  const shown = useMemo(() => (models ?? []).filter((m) => !q || m.label.toLowerCase().includes(q.toLowerCase())), [models, q]);
  const custom = q && !(models ?? []).some((m) => m.id === q) ? q : null;
  const phaseIndex = handoff ? PHASES.findIndex(([p]) => p === handoff.phase) : -1;

  return (
    <div className="grid gap-3">
      <fieldset disabled={running} className="grid gap-3">
        <div className="grid grid-cols-2 gap-2">
          <Label text="Provider">
            <select className={FIELD} data-testid="sm-provider" value={provider} onChange={(e) => setProvider(e.target.value)}>
              {(providers.length ? providers : [{ id: "claude", label: "Claude" }]).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </Label>
          <Label text="Account">
            <select className={FIELD} data-testid="sm-account" value={account} disabled={!hasAccounts} onChange={(e) => setAccount(e.target.value)}>
              <option value="">Default</option>
              {accounts.map((a) => (
                <option key={a.name} value={a.name}>
                  {a.email ? `${a.name} (${a.email})` : a.name}
                </option>
              ))}
            </select>
          </Label>
        </div>
        <Label text="Model">
          <input className={FIELD} data-testid="sm-query" placeholder="Search or type a model id" value={query} onChange={(e) => setQuery(e.target.value)} />
        </Label>
        <div role="listbox" aria-label="Models" data-testid="sm-models" className="max-h-[160px] overflow-y-auto rounded-md border border-border">
          {!models && <p className="px-2 py-1.5 text-sm text-muted-foreground">Reading models…</p>}
          {shown.map((m) => (
            <button key={m.id} type="button" role="option" aria-selected={model === m.id} className="flex w-full items-center gap-2 px-2 py-1 text-left text-sm hover:bg-hover aria-selected:bg-selected" onClick={() => setModel(m.id)}>
              <span className="min-w-0 flex-1 truncate">{m.label}</span>
              {model === m.id && <CheckIcon className="size-3.5" />}
            </button>
          ))}
          {custom && (
            <button type="button" role="option" aria-selected={model === custom} data-testid="sm-custom" className="flex w-full items-center gap-2 px-2 py-1 text-left text-sm hover:bg-hover aria-selected:bg-selected" onClick={() => setModel(custom)}>
              Use “{custom}”
            </button>
          )}
        </div>
      </fieldset>
      {handoff && (
        <ol className="grid gap-1 text-sm" data-testid="sm-progress" data-phase={handoff.phase} aria-label="Progress">
          {PHASES.map(([p, label], i) => {
            const now = handoff.phase === p;
            const done = phaseIndex > i || handoff.phase === "completed";
            return (
              <li key={p} className={`flex items-center gap-2 ${now ? "text-foreground" : "text-muted-foreground"}`}>
                {now && running ? <LoaderCircleIcon className="size-3.5 animate-spin" /> : done ? <CheckIcon className="size-3.5 text-done" /> : <CircleIcon className="size-3.5" />}
                {label}
              </li>
            );
          })}
          {handoff.phase === "cancelled" && (
            <li className="flex items-center gap-2 text-muted-foreground">
              <XIcon className="size-3.5" /> Cancelled. This chat is unchanged.
            </li>
          )}
        </ol>
      )}
      <ErrorLine text={handoff?.error ?? error} />
      <div className="flex justify-end gap-2">
        {running ? (
          <Button type="button" variant="outline" size="sm" data-testid="sm-cancel" disabled={!handoff.canCancel} onClick={() => void app.life.cancelSwitch()}>
            Cancel switch
          </Button>
        ) : (
          <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("switchModel")}>
            Close
          </Button>
        )}
        <Button type="button" size="sm" data-testid="sm-start" disabled={!model || running} onClick={() => model && void app.life.switchModel(old, provider, model, account || null)}>
          {running && <LoaderCircleIcon className="animate-spin" />}
          Switch
        </Button>
      </div>
    </div>
  );
}

// ---- confirm ----

function ConfirmDialog({ app }: { app: App }) {
  const confirm = useLife(app, "confirm");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => setError(null), [confirm]);
  return (
    <Frame open={confirm !== null} onClose={() => !busy && app.life.close("confirm")} title={confirm?.title ?? ""} testid="confirm">
      {confirm && (
        <>
          <p className="text-sm break-words whitespace-pre-wrap select-text" data-testid="confirm-body">
            {confirm.body}
          </p>
          <ErrorLine text={error} />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={busy} onClick={() => app.life.close("confirm")}>
              Cancel
            </Button>
            <Button
              type="button"
              size="sm"
              data-testid="confirm-action"
              className="bg-failed text-white hover:bg-failed/90"
              disabled={busy || confirm.blocked}
              onClick={async () => {
                setBusy(true);
                setError(await app.life.confirmed());
                setBusy(false);
              }}
            >
              {busy && <LoaderCircleIcon className="animate-spin" />}
              {confirm.action}
            </Button>
          </div>
        </>
      )}
    </Frame>
  );
}

// ---- Recently closed ----

/** "5m", "3h", "2d" since a Unix time in seconds. */
function ageText(seconds: number, now = Date.now() / 1000): string {
  const s = Math.max(0, now - seconds);
  if (s < 3600) return `${Math.max(1, Math.floor(s / 60))}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

function ClosedDialog({ app }: { app: App }) {
  const open = useLife(app, "closed");
  return (
    <Frame open={open} onClose={() => app.life.close("closed")} title="Recently closed" description="Archived chats. Open one to bring it back." testid="recently-closed">
      {open && <ClosedBody app={app} />}
    </Frame>
  );
}

function ClosedBody({ app }: { app: App }) {
  const [chats, setChats] = useState<ClosedChat[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    app.life.closedChats().then(setChats, (err) => setError(message(err)));
  }, [app]);
  const reopen = async (id: string) => {
    setBusy(true);
    try {
      await app.life.reopen(id);
    } catch (err) {
      setError(message(err));
    }
    setBusy(false);
  };
  return (
    <>
      <div role="list" className="grid max-h-[320px] overflow-y-auto">
        {!chats && !error && <p className="text-sm text-muted-foreground">Reading closed chats…</p>}
        {chats?.length === 0 && <p className="text-sm text-muted-foreground">No archived chats.</p>}
        {chats?.map((c) => (
          <button
            key={c.id}
            role="listitem"
            type="button"
            disabled={busy}
            data-testid="closed-chat"
            data-session={c.id}
            title={c.cwd}
            className="grid gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-hover focus-visible:bg-selected focus-visible:outline-none"
            onClick={() => void reopen(c.id)}
          >
            <span className="truncate text-sm font-medium">
              {c.cwd.split("/").pop()} <span className="font-mono text-xs text-muted-foreground">{c.id.slice(0, 8)}</span>
            </span>
            <span className="truncate text-xs text-muted-foreground">
              {c.chair_provider ?? "chat"} · closed {ageText(c.archivedAt)} ago · {c.cwd}
            </span>
          </button>
        ))}
      </div>
      <ErrorLine text={error} />
      <div className="flex justify-end">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("closed")}>
          Close
        </Button>
      </div>
    </>
  );
}
