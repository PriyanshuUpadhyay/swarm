import { Component, lazy, Suspense, useEffect, useState, useSyncExternalStore, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { usePanelRef, useDefaultLayout } from "react-resizable-panels";
import { toast } from "sonner";
import { showError, shownErrors } from "./errors";
import { CopyIcon } from "lucide-react";
import { forgetToken } from "../core/api";
import type { App } from "../core/app";
import { boot, NO_TOKEN, savedStore } from "../core/boot";
import { shortId } from "../core/route";
import { Changes } from "../core/changes";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "../components/ui/resizable";
import { Toaster } from "../components/ui/sonner";
import { TooltipProvider } from "../components/ui/tooltip";
import "../styles.css";
import { PanelToggles, WorkspacePanel } from "./changes";
import { AskDialog, Home, WorkspaceLanding } from "./home";
import { LifecycleDialogs } from "./lifecycle";
import { SettingsDialog } from "./settings";
import { Palette } from "./palette";
import { PaneStrip } from "./panes";
import { Sidebar } from "./sidebar";
import { store } from "./store";
import { TabStrip } from "./tabs";
import { applyAppearance, chooseTheme, followAppearance, useTheme } from "./appearance";
import { copyText } from "./copy";

// Sidebar side and width stay in this browser until P3b adds their owner choices.
const SIDE_KEY = "swarm.sidebar.side";
// ⌘B and ⇧⌘B are registered before the panel exists; Root points them at it.
const frame = { toggle: () => {}, move: () => {} };
// ⌘J likewise; ChatColumn points it at the background panel. Open or not stays in this browser.
const backgroundPanel = { toggle: () => {} };
const BACKGROUND_KEY = "swarm.background.open";
// xterm is large, so the panel loads at its first ⌘J.
const BackgroundPanel = lazy(() => import("./background").then((m) => ({ default: m.BackgroundPanel })));

function Root({ app, changes }: { app: App; changes: Changes }) {
  useEffect(() => app.start(), [app]);
  useEffect(() => followAppearance(app), [app]);
  const theme = useTheme();
  const sidebar = usePanelRef();
  const [side, setSide] = useState(store.getItem(SIDE_KEY) === "right" ? "right" : "left");
  useEffect(() => {
    frame.toggle = () => {
      const panel = sidebar.current;
      if (panel?.isCollapsed()) panel.expand();
      else panel?.collapse();
    };
    frame.move = () =>
      setSide((s) => {
        const next = s === "left" ? "right" : "left";
        store.setItem(SIDE_KEY, next);
        return next;
      });
  }, [sidebar]);
  const layout = useDefaultLayout({ id: "swarm.layout", storage: store, panelIds: ["side-panel", "main-panel"] });

  const side_ = (
    <ResizablePanel
      key="side"
      id="side-panel"
      panelRef={sidebar}
      defaultSize="260px"
      minSize="180px"
      maxSize="560px"
      collapsible
      groupResizeBehavior="preserve-pixel-size"
    >
      <Sidebar app={app} />
    </ResizablePanel>
  );
  const main = (
    <ResizablePanel key="main" id="main-panel" minSize="360px">
      <Main app={app} changes={changes} />
    </ResizablePanel>
  );
  return (
    <TooltipProvider>
      <ResizablePanelGroup key={side} orientation="horizontal" data-side={side} defaultLayout={layout.defaultLayout} onLayoutChanged={layout.onLayoutChanged}>
        {side === "left" ? [side_, <ResizableHandle key="h" />, main] : [main, <ResizableHandle key="h" />, side_]}
      </ResizablePanelGroup>
      <Palette app={app} />
      <AskDialog app={app} />
      <LifecycleDialogs app={app} />
      <SettingsDialog app={app} />
      <Toaster theme={theme} position="bottom-right" />
    </TooltipProvider>
  );
}

/** The open chat, else the selected workspace's landing, else Home; the workspace panel beside it. */
function Main({ app, changes }: { app: App; changes: Changes }) {
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const workspace = useSyncExternalStore(app.subscribe, () => app.view.workspace);
  return (
    <div className="flex h-full min-w-0 flex-col">
      {(workspace || openId) && (
        <div className="flex min-w-0">
          <div className="min-w-0 flex-1">{workspace && <TabStrip app={app} />}</div>
          {openId && <ChatId id={openId} short={shortId(openId, app.chatIds())} />}
          <PanelToggles changes={changes} />
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">{openId ? <ChatColumn app={app} openId={openId} /> : workspace ? <WorkspaceLanding app={app} /> : <Home app={app} />}</div>
        <WorkspacePanel app={app} changes={changes} />
      </div>
    </div>
  );
}

/** Catches a failed load or render of the lazy panel, so the rest of the page stays. Keyed by
 *  chat, so the next chat starts clean. */
class LoadFailed extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? (
      <p role="alert" className="p-3 text-sm text-muted-foreground">
        The background panel failed. Reload the page to try again.
      </p>
    ) : (
      this.props.children
    );
  }
}

/** The open chat over the ⌘J panel of its agents and background tasks (ADR 0102). */
function ChatColumn({ app, openId }: { app: App; openId: string }) {
  const [open, setOpen] = useState(() => store.getItem(BACKGROUND_KEY) === "1");
  useEffect(() => {
    backgroundPanel.toggle = () =>
      setOpen((was) => {
        store.setItem(BACKGROUND_KEY, was ? "0" : "1");
        return !was;
      });
    return () => {
      backgroundPanel.toggle = () => {};
    };
  }, []);
  // The ids must be the panels that mount, so each set keeps its own saved layout.
  const layout = useDefaultLayout({ id: "swarm.background", storage: store, panelIds: open ? ["chat-panel", "background-panel"] : ["chat-panel"] });
  return (
    <ResizablePanelGroup orientation="vertical" defaultLayout={layout.defaultLayout} onLayoutChanged={layout.onLayoutChanged}>
      <ResizablePanel id="chat-panel" minSize="160px">
        <div className="flex h-full min-h-0 flex-col">
          <PaneStrip app={app} />
        </div>
      </ResizablePanel>
      {open && <ResizableHandle className="h-px w-full after:inset-x-0 after:inset-y-auto after:top-1/2 after:left-0 after:h-[7px] after:w-full after:translate-x-0 after:-translate-y-1/2" />}
      {open && (
        <ResizablePanel id="background-panel" defaultSize="280px" minSize="120px">
          <LoadFailed key={openId}>
            <Suspense fallback={<p role="status" className="p-3 text-sm text-muted-foreground">Loading…</p>}>
              <BackgroundPanel app={app} session={openId} />
            </Suspense>
          </LoadFailed>
        </ResizablePanel>
      )}
    </ResizablePanelGroup>
  );
}

/** The open chat's short id, as the URL and `swarm` name it; a click copies the full id. */
function ChatId({ id, short }: { id: string; short: string }) {
  return (
    <button
      type="button"
      className="flex shrink-0 items-center gap-1 self-center px-2 font-mono text-xs text-muted-foreground hover:text-foreground"
      aria-label={`Copy chat id ${id}`}
      title="Copy chat id"
      onClick={() => void copyText(id, "chat id")}
    >
      {short}
      <CopyIcon className="size-3" aria-hidden />
    </button>
  );
}

applyAppearance();
const app = boot();
const changes = typeof app === "string" ? null : new Changes(app);
if (typeof app !== "string") {
  app.registry.add(...changes!.commands());
  app.notice = (text, error) => (error ? showError(text) : toast(text));
  // A failed report is dropped, so it never shows an error of its own.
  shownErrors.report = (text) => void app.api.post("/api/ui-error", { text, page: location.pathname }).catch(() => {});
  // A token that the server refuses is forgotten, so the next link that swarm ui opens saves its own.
  app.unauthorized = () => {
    forgetToken(savedStore());
    root.render(<p className="p-6 text-muted-foreground">{NO_TOKEN}</p>);
  };
  app.registry.add(
    { id: "sidebar.toggle", title: "Toggle Sidebar", chord: "Meta+B", run: () => frame.toggle() },
    { id: "sidebar.move", title: "Move Sidebar to Other Side", chord: "Shift+Meta+B", run: () => frame.move() },
    { id: "panel.background", title: "Toggle Agents and Background Tasks", chord: "Meta+J", when: () => app.openId !== null, run: () => backgroundPanel.toggle() },
  );
  for (const t of ["system", "light", "dark"] as const)
    app.registry.add({ id: `theme.${t}`, title: `Theme: ${t[0].toUpperCase()}${t.slice(1)}`, run: () => chooseTheme(app, t) });
}
const root = createRoot(document.getElementById("root")!);
root.render(typeof app === "string" ? <p className="p-6 text-muted-foreground">{app}</p> : <Root app={app} changes={changes!} />);
