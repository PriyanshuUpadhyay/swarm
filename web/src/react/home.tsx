// Home (HomeView), the workspace landing (SessionsWindow.workspaceLanding), and the one name dialog.

import { CircleCheckIcon, CircleIcon } from "lucide-react";
import { useEffect, useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import type { Load } from "../core/changes";
import { workspaceRow } from "../core/nav";
import { hudCard, USAGE_PROVIDERS, type Usage } from "../core/settings";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { StatusGlyph } from "./status";
import { NoticesCard } from "./notices";

export function Home({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  const recent = view.recent.slice(0, 8);
  const hasProject = (view.tree?.sections.length ?? 0) > 0;
  const hasChat = recent.length > 0;
  const hasAccount = useSyncExternalStore(app.settings.subscribe, () => signedIn(app.settings.state.usage));
  const hud = useSyncExternalStore(app.settings.subscribe, () => app.settings.state.hud);
  const hudPending = useSyncExternalStore(app.settings.subscribe, () => app.settings.state.hudPending);
  // Fire-and-forget is safe: loadHud catches its own errors into the hud state.
  useEffect(() => void app.settings.loadHud(), [app]);
  // loadUsage keeps its errors in state.usage, so nothing waits for it.
  useEffect(() => {
    if (app.settings.state.usage.state === "idle") void app.settings.loadUsage();
  }, [app]);
  const card = hudCard(hud, view.loaded, view.tree?.prefs.usageHUDAsked);
  const answer = (add: boolean) => {
    if (hudPending) return;
    // Fire-and-forget is safe: choose shows a refused write as a notice, and setHud posts its own.
    void app.settings.setPref("usageHUDAsked", true);
    if (add) void app.settings.setHud(true);
  };
  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-testid="home">
      <div className="mx-auto grid max-w-[760px] gap-8 px-8 py-8">
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => app.life.openProject("import")}>
            Import Project…
          </Button>
          <Button variant="outline" size="sm" onClick={() => app.life.openProject("create")}>
            Create Project…
          </Button>
        </div>
        <NoticesCard app={app} />
        {card !== "none" && (
          <section
            className={`grid gap-2 rounded-md border border-border px-4 py-3${card === "hold" ? " invisible" : ""}`}
            aria-label="Usage HUD"
            aria-hidden={card === "hold" || undefined}
            data-testid={card === "show" ? "hud-card" : undefined}
          >
            <h2 className="font-semibold">Add the usage HUD?</h2>
            <p className="text-sm text-muted-foreground">See Claude and Codex limits at the notch. It starts at login; you can turn it off in Settings › Accounts.</p>
            <div className="flex gap-2">
              {/* aria-disabled, not disabled: a disabled button drops keyboard focus to the page body. */}
              <Button size="sm" data-testid="hud-add" aria-disabled={hudPending || undefined} onClick={() => answer(true)}>
                {hudPending ? "Adding…" : "Add"}
              </Button>
              <Button size="sm" variant="ghost" data-testid="hud-skip" aria-disabled={hudPending || undefined} onClick={() => answer(false)}>
                Not now
              </Button>
            </div>
            <p role="status" className="sr-only">
              {hudPending ? "Adding the usage HUD…" : ""}
            </p>
          </section>
        )}
        {view.error && <p className="text-sm text-failed select-text">Chats unavailable: {view.error}</p>}
        {(!hasProject || !hasChat) && (
          <section className="grid gap-3" aria-label="Get started">
            <h2 className="text-lg font-semibold">Get started</h2>
            <Step done={hasProject} title="Import a project" run={() => app.life.openProject("import")} />
            <Step done={hasAccount} title="Sign in to an account" run={() => app.settings.show("accounts")} />
            <Step done={hasChat} title="Start a chat" run={hasProject ? () => app.life.newWorkspace() : undefined} />
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
                <span className="truncate text-xs text-muted-foreground">{r.project && r.project !== r.workspace ? `${r.project} · ${r.workspace}` : r.workspace}</span>
              </span>
              <span className="text-xs text-muted-foreground tabular-nums">{r.age}</span>
            </button>
          ))}
        </section>
      </div>
    </div>
  );
}

/** A first-run step: its action, or the phase that brings it. */
/** A Claude or Codex account with a sign-in, from the cached meters the sidebar footer reads. */
function signedIn(usage: Load<Usage>): boolean {
  return usage.state === "ready" && usage.data.meters.some((m) => USAGE_PROVIDERS.includes(m.provider) && m.state !== "no_source" && m.reason_code !== "no_credentials");
}

function Step({ done, title, run }: { done: boolean; title: string; run?: () => void }) {
  return (
    <div className="flex items-center gap-3">
      {done ? <CircleCheckIcon className="size-4 text-done" aria-label="Done" /> : <CircleIcon className="size-4 text-muted-foreground" aria-label="To do" />}
      <Button variant="ghost" size="sm" disabled={!run} onClick={run}>
        {title}
      </Button>
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
      <p className="text-muted-foreground">{view.tabs?.tabs.some((tab) => tab.pending === null) ? "Pick a tab, or start a new chat." : "This workspace has no open chats."}</p>
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
