// Settings (⌘,) with five pages, and the compact usage meters in the sidebar footer (S5). Rows are
// plain elements with inline buttons; no row has its own menu, tooltip or hover card.

import { TriangleAlertIcon } from "lucide-react";
import { useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { CARRY_OPTIONS, savedCarry } from "../core/composer";
import type { Prefs } from "../core/nav";
import { PAGES, type Page } from "../core/settings";
import { Button } from "../components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";
import { chooseTheme, fontMono, fontSans, useTheme, type FontMono, type FontSans, type Theme } from "./appearance";
import { label, Row, Segmented, Toggle, useSettings } from "./settings-shared";
import { AccountsPage } from "./settings-accounts";
import { ProfilesPage } from "./settings-profiles";
import { SetupPage } from "./settings-setup";

export { UsageMeters } from "./usage-meters";

// ---- dialog ----

export function SettingsDialog({ app }: { app: App }) {
  const open = useSettings(app, "open");
  const page = useSettings(app, "page");
  return (
    <Dialog open={open} onOpenChange={(o) => !o && app.settings.close()}>
      {open && (
        <DialogContent className="flex h-[min(600px,calc(100vh-104px))] w-[min(780px,calc(100vw-32px))] p-0" data-testid="settings" aria-describedby={undefined}>
          <nav aria-label="Settings pages" className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-border p-2">
            <DialogTitle className="px-2 pt-1 pb-2">Settings</DialogTitle>
            {PAGES.map(([id, title]) => (
              <button
                key={id}
                className={`rounded-md px-2 py-1.5 text-left text-sm ${page === id ? "bg-selected text-foreground" : "text-muted-foreground hover:bg-hover hover:text-foreground"}`}
                aria-current={page === id ? "page" : undefined}
                data-testid={`settings-page-${id}`}
                onClick={() => app.settings.show(id)}
              >
                {title}
              </button>
            ))}
          </nav>
          <div className="grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)] content-start gap-4 overflow-y-auto p-5" data-testid={`settings-${page}`}>
            <h2 className="text-lg font-semibold">{PAGES.find(([id]) => id === page)![1]}</h2>
            <Banner app={app} />
            <PageBody app={app} page={page} />
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}

function PageBody({ app, page }: { app: App; page: Page }) {
  if (page === "appearance") return <AppearancePage app={app} />;
  if (page === "notifications") return <NotificationsPage app={app} />;
  if (page === "profiles") return <ProfilesPage app={app} />;
  if (page === "setup") return <SetupPage app={app} />;
  return <AccountsPage app={app} />;
}

/** The refused write, the login note, or a destructive action waiting for its second press. */
function Banner({ app }: { app: App }) {
  const error = useSettings(app, "error");
  const login = useSettings(app, "login");
  const confirm = useSettings(app, "confirm");
  return (
    <>
      {error && (
        <p role="alert" data-testid="settings-error" className="flex items-start gap-1.5 text-sm break-words text-failed select-text">
          <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" />
          {error}
        </p>
      )}
      {login && (
        <p role="status" data-testid="login-note" className="rounded-md border border-border-strong px-3 py-2 text-sm">
          Finish signing in to {label(login.provider)} as “{login.account}” in the login pane <code className="font-mono">{login.pane}</code>. Then press Refresh accounts.
        </p>
      )}
      {confirm && (
        <div role="alertdialog" aria-label={confirm.action} data-testid="settings-confirm" className="grid gap-2 rounded-md border border-border-strong px-3 py-2 text-sm">
          <p>{confirm.text}</p>
          <div className="flex gap-2">
            <Button size="sm" data-testid="settings-confirm-yes" onClick={() => void app.settings.answer(true)}>
              {confirm.action}
            </Button>
            <Button size="sm" variant="outline" onClick={() => void app.settings.answer(false)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </>
  );
}

/** The owner's prefs from the last tree; a write shows once the tree after it arrives. */
function usePrefs(app: App): Prefs {
  return useSyncExternalStore(app.subscribe, () => app.view.tree?.prefs ?? NO_PREFS);
}
const NO_PREFS: Prefs = {};

function AppearancePage({ app }: { app: App }) {
  const theme = useTheme();
  const sans = fontSans.useChoice();
  const mono = fontMono.useChoice();
  const prefs = usePrefs(app);
  return (
    <div className="grid gap-4">
      <Row title="Theme">
        <Segmented<Theme>
          label="Theme"
          testid="pref-theme"
          value={theme}
          options={[
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ]}
          onChange={(t) => chooseTheme(app, t)}
        />
      </Row>
      <Row title="UI font">
        <Segmented<FontSans>
          label="UI font"
          testid="pref-font-sans"
          value={sans}
          options={[
            ["system", "System"],
            ["helvetica", "Helvetica"],
            ["avenir", "Avenir"],
          ]}
          onChange={(f) => fontSans.choose(app, f)}
        />
      </Row>
      <Row title="Code font">
        <Segmented<FontMono>
          label="Code font"
          testid="pref-font-mono"
          value={mono}
          options={[
            ["system", "System"],
            ["menlo", "Menlo"],
            ["monaco", "Monaco"],
          ]}
          onChange={(f) => fontMono.choose(app, f)}
        />
      </Row>
      {/* Swarm.app owns zoom in its View menu, so the page has no zoom control of its own. */}
      <div data-testid="zoom-hint">
        <div className="text-sm">Zoom</div>
        <div className="text-xs text-muted-foreground">In Swarm.app, use View › Zoom In, Zoom Out or Actual Size (⌘+, ⌘−, ⌘0). In a browser, use the browser's zoom.</div>
      </div>
      <Row title="Send with" detail="The other key adds a new line.">
        <Segmented
          label="Send with"
          testid="pref-send-key"
          value={prefs.sendKey === "commandReturn" ? "commandReturn" : "return"}
          options={[
            ["return", "Return"],
            ["commandReturn", "⌘Return"],
          ]}
          onChange={(v) => void app.settings.setPref("sendKey", v)}
        />
      </Row>
      <Row title="Switch to another CLI" detail="What the new chat gets. The run chip asks each time with this as the default.">
        <Segmented
          label="Switch to another CLI"
          testid="pref-switch-carry"
          value={savedCarry(prefs)}
          options={CARRY_OPTIONS}
          onChange={(v) => void app.settings.setPref("switchCarry", v)}
        />
      </Row>
    </div>
  );
}

const NOTICES: [keyof NonNullable<Prefs["notices"]> & string, string, string][] = [
  ["post", "Post notices", "A chat that waits for you posts a notice."],
  ["sound", "Play a sound", ""],
  ["done", "Notify when a chat is done", ""],
  ["badge", "Badge the Dock", "The Dock shows the number of chats that need input."],
];

function NotificationsPage({ app }: { app: App }) {
  const prefs = usePrefs(app);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const notices = prefs.notices ?? {};
  const muted = notices.mutedProjects ?? [];
  const projects = (tree?.sections ?? []).filter((s) => s.kind === "project" && s.path);
  return (
    <div className="grid gap-4">
      {NOTICES.map(([key, title, detail]) => (
        <Row key={key} title={title} detail={detail || undefined}>
          <Toggle label={title} testid={`notice-${key}`} checked={notices[key] !== false} onChange={(on) => void app.settings.setPref(`notices.${key}`, on)} />
        </Row>
      ))}
      <h3 className="mt-2 text-sm font-semibold">Mute projects</h3>
      {projects.length === 0 && <p className="text-sm text-muted-foreground">Import a project to set its mute choice.</p>}
      {projects.map((p) => (
        <Row key={p.path} title={p.title} detail={p.path!}>
          <Toggle label={`Mute ${p.title}`} testid="mute-project" checked={muted.includes(p.path!)} onChange={(on) => void app.settings.mute(p.path!, on)} />
        </Row>
      ))}
    </div>
  );
}
