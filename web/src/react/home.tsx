// Home (HomeView), the workspace landing (SessionsWindow.workspaceLanding), and the one name dialog.

import { CircleCheckIcon, CircleIcon } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { workspaceRow } from "../core/nav";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { StatusGlyph } from "./status";

export function Home({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  const recent = view.recent.slice(0, 8);
  const hasProject = (view.tree?.sections.length ?? 0) > 0;
  const hasChat = recent.length > 0;
  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-testid="home">
      <div className="mx-auto grid max-w-[760px] gap-8 px-8 py-8">
        <div className="flex gap-2">
          <Button variant="outline" size="sm" disabled title="Comes in S3">
            Import Project… <span className="text-faint-foreground">S3</span>
          </Button>
          <Button variant="outline" size="sm" disabled title="Comes in S3">
            Create Project… <span className="text-faint-foreground">S3</span>
          </Button>
        </div>
        {view.error && <p className="text-sm text-failed select-text">Chats unavailable: {view.error}</p>}
        {(!hasProject || !hasChat) && (
          <section className="grid gap-3" aria-label="Get started">
            <h2 className="text-lg font-semibold">Get started</h2>
            <Step done={hasProject} title="Import a project" phase="S3" />
            <Step done={false} title="Run setup" phase="S5" />
            <Step done={hasChat} title="Start a chat" phase="S3" />
          </section>
        )}
        <section className="grid gap-1" aria-label="Recent work">
          <h1 className="mb-2 text-2xl font-bold">Recent work</h1>
          {recent.length === 0 && <p className="text-muted-foreground">Your recent chats appear here.</p>}
          {recent.map((r) => (
            <button
              key={r.session}
              data-testid="recent-chat"
              title={r.workspacePath}
              onClick={() => app.openChat(r.session)}
              className="flex min-h-9 items-center gap-3 border-b border-border px-1 py-1.5 text-left hover:bg-hover"
            >
              <span className="grid w-4 place-items-center">{r.status && <StatusGlyph status={r.status} />}</span>
              <span className="grid min-w-0 flex-1">
                <span className="truncate font-medium">{r.title}</span>
                <span className="truncate text-xs text-muted-foreground">{r.workspace}</span>
              </span>
              <span className="text-xs text-muted-foreground tabular-nums">{r.age}</span>
            </button>
          ))}
        </section>
      </div>
    </div>
  );
}

/** A first-run step; its action comes in a later phase. */
function Step({ done, title, phase }: { done: boolean; title: string; phase: string }) {
  return (
    <div className="flex items-center gap-3">
      {done ? <CircleCheckIcon className="size-4 text-done" aria-label="Done" /> : <CircleIcon className="size-4 text-muted-foreground" aria-label="To do" />}
      <Button variant="ghost" size="sm" disabled title={`Comes in ${phase}`}>
        {title}
      </Button>
      <span className="text-xs text-faint-foreground">comes in {phase}</span>
    </div>
  );
}

/** A selected workspace with no chat open. */
export function WorkspaceLanding({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  const row = workspaceRow(view.tree, view.workspace);
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4" data-testid="workspace-landing">
      <h2 className="text-lg font-semibold">{row?.title ?? view.workspace?.split("/").pop()}</h2>
      <p className="text-muted-foreground">This workspace has no open chats.</p>
      <Button size="sm" disabled={!row?.newChatEnabled} onClick={() => void app.startChat()}>
        New chat
      </Button>
      {row && !row.newChatEnabled && <p className="text-sm text-muted-foreground">{row.archived ? "This workspace is archived." : "This workspace is read-only."}</p>}
    </div>
  );
}

/** The app's one name dialog. */
export function AskDialog({ app }: { app: App }) {
  const ask = useSyncExternalStore(app.subscribe, () => app.view.asking);
  return (
    <Dialog open={ask !== null} onOpenChange={(o) => !o && app.ask(null)}>
      {ask && (
        <DialogContent className="w-[min(400px,calc(100vw-32px))] p-4" aria-describedby={undefined}>
          <AskBody app={app} key={ask.title + ask.value} />
        </DialogContent>
      )}
    </Dialog>
  );
}

function AskBody({ app }: { app: App }) {
  const ask = app.view.asking!;
  const [name, setName] = useState(ask.value);
  // An empty rename clears the saved name.
  const save = () => {
    app.ask(null);
    ask.save(name.trim());
  };
  return (
    <form
      className="grid gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <DialogTitle>{ask.title}</DialogTitle>
      <input
        autoFocus
        aria-label="Name"
        data-testid="ask-name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onFocus={(e) => e.target.select()}
        className="h-8 rounded-md border border-border-strong bg-background px-2 outline-none focus:border-ring"
      />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => app.ask(null)}>
          Cancel
        </Button>
        <Button type="submit" size="sm" data-testid="ask-save">
          Save
        </Button>
      </div>
    </form>
  );
}
