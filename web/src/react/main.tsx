import { useEffect, useState, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { usePanelRef, useDefaultLayout } from "react-resizable-panels";
import { toast } from "sonner";
import type { App } from "../core/app";
import { boot } from "../core/boot";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "../components/ui/resizable";
import { Toaster } from "../components/ui/sonner";
import { TooltipProvider } from "../components/ui/tooltip";
import "../styles.css";
import { AskDialog, Home, WorkspaceLanding } from "./home";
import { LifecycleDialogs } from "./lifecycle";
import { Palette } from "./palette";
import { PaneStrip } from "./panes";
import { Sidebar } from "./sidebar";
import { store } from "./store";
import { TabStrip } from "./tabs";
import { applyTheme, setTheme, useTheme } from "./theme";

// Sidebar side and width stay in this browser until P3b adds their owner choices.
const SIDE_KEY = "swarm.sidebar.side";
// ⌘B and ⇧⌘B are registered before the panel exists; Root points them at it.
const frame = { toggle: () => {}, move: () => {} };

function Root({ app }: { app: App }) {
  useEffect(() => app.start(), [app]);
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
      <Main app={app} />
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
      <Toaster theme={theme} position="bottom-right" />
    </TooltipProvider>
  );
}

/** The open chat, else the selected workspace's landing, else Home. */
function Main({ app }: { app: App }) {
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const workspace = useSyncExternalStore(app.subscribe, () => app.view.workspace);
  return (
    <div className="flex h-full min-w-0 flex-col">
      {workspace && <TabStrip app={app} />}
      {openId ? <PaneStrip app={app} /> : workspace ? <WorkspaceLanding app={app} /> : <Home app={app} />}
    </div>
  );
}

applyTheme();
const app = boot();
if (typeof app !== "string") {
  app.notice = (text, error) => (error ? toast.error(text) : toast(text));
  app.registry.add(
    { id: "sidebar.toggle", title: "Toggle Sidebar", chord: "Meta+B", run: () => frame.toggle() },
    { id: "sidebar.move", title: "Move Sidebar to Other Side", chord: "Shift+Meta+B", run: () => frame.move() },
  );
  for (const t of ["system", "light", "dark"] as const)
    app.registry.add({ id: `theme.${t}`, title: `Theme: ${t[0].toUpperCase()}${t.slice(1)}`, run: () => setTheme(t) });
}
const root = createRoot(document.getElementById("root")!);
root.render(typeof app === "string" ? <p className="p-6 text-muted-foreground">{app}</p> : <Root app={app} />);
