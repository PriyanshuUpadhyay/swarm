// Project creation, folder import, and project setup scripts.

import { useEffect, useRef, useState } from "react";
import type { App } from "../core/app";
import { Button } from "../components/ui/button";
import { FIELD, message, useLife, Frame, Label, ErrorLine } from "./lifecycle-shared";

declare global {
  interface Window {
    /** The Mac shell's one script message (ADR 0077); a plain browser has none. */
    webkit?: { messageHandlers?: { swarm?: { postMessage: (body: { kind: "pickFolder" }) => Promise<string | null> } } };
  }
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
  const existing = useLife(app, "existing");
  const [path, setPath] = useState(() => app.life.state.projectPath);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Create on a folder that exists offers Import until the path changes.
  const offer = existing !== null && existing === path.trim();
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
      {offer ? (
        <div className="grid gap-2" data-testid="project-exists">
          <p className="text-sm">This folder already exists. To add it to swarm, import it. To make a new project, type a path that does not exist yet.</p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("project")}>
              Cancel
            </Button>
            <Button type="button" size="sm" onClick={() => app.life.openProject("import", existing)}>
              Import this folder
            </Button>
          </div>
        </div>
      ) : (
        <ErrorLine text={error} />
      )}
      {offer ? null : choice !== null ? (
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
  const [attempt, setAttempt] = useState(0);
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  const loaded = script !== null;
  useEffect(() => {
    if (loaded) field.current?.focus();
  }, [loaded]);
  useEffect(() => {
    let live = true;
    setError(null);
    app.life.options(project).then(
      (o) => live && setScript(o.setup_script ?? ""),
      (err) => live && setError(message(err)),
    );
    return () => { live = false; };
  }, [app, project, attempt]);
  const save = async () => {
    if (script === null || busy) return;
    setBusy(true);
    setError(null);
    try {
      if (!await app.life.saveSetupScript(project, script)) setError("Could not save. Your draft is kept. Try Save again.");
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <form
      className="grid gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <textarea
        ref={field}
        data-autofocus
        aria-label="Setup script"
        data-testid="setup-script"
        rows={5}
        disabled={script === null || busy}
        className="w-full resize-y rounded-md border border-border-strong bg-background p-2 font-mono text-sm outline-none focus:border-ring"
        placeholder="pnpm install"
        value={script ?? ""}
        onChange={(e) => setScript(e.target.value)}
      />
      <ErrorLine text={error} />
      {script === null && error && <Button type="button" variant="outline" size="sm" onClick={() => setAttempt(attempt + 1)}>Retry</Button>}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.life.close("setupScript")}>
          Cancel
        </Button>
        <Button type="submit" size="sm" data-testid="setup-script-save" disabled={script === null || busy}>
          Save
        </Button>
      </div>
    </form>
  );
}

export { ProjectDialog, SetupScriptDialog };
