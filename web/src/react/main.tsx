import { useEffect } from "react";
import { createRoot } from "react-dom/client";
import { usePanelRef, useDefaultLayout } from "react-resizable-panels";
import type { App } from "../core/app";
import { boot } from "../core/boot";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "../components/ui/resizable";
import { Toaster } from "../components/ui/sonner";
import { TooltipProvider } from "../components/ui/tooltip";
import "../styles.css";
import { Palette } from "./palette";
import { PaneStrip } from "./panes";
import { Sidebar } from "./sidebar";
import { store } from "./store";
import { TabStrip } from "./tabs";
import { applyTheme, setTheme, useTheme } from "./theme";

// Only the left side for now; P2 adds the command that moves it.
const side = store.getItem("swarm.sidebar.side") === "right" ? "right" : "left";
// The ⌘B command is registered before the panel exists; Root points it at the panel.
const sidebarToggle = { run: () => {} };

function Root({ app }: { app: App }) {
  useEffect(() => app.start(), [app]);
  const theme = useTheme();
  const sidebar = usePanelRef();
  useEffect(() => {
    sidebarToggle.run = () => {
      const panel = sidebar.current;
      if (panel?.isCollapsed()) panel.expand();
      else panel?.collapse();
    };
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
      <div className="flex h-full min-w-0 flex-col">
        <TabStrip app={app} />
        <PaneStrip app={app} />
      </div>
    </ResizablePanel>
  );
  return (
    <TooltipProvider>
      <ResizablePanelGroup orientation="horizontal" defaultLayout={layout.defaultLayout} onLayoutChanged={layout.onLayoutChanged}>
        {side === "left" ? [side_, <ResizableHandle key="h" />, main] : [main, <ResizableHandle key="h" />, side_]}
      </ResizablePanelGroup>
      <Palette app={app} />
      <Toaster theme={theme} position="bottom-right" />
    </TooltipProvider>
  );
}

applyTheme();
const app = boot();
if (typeof app !== "string") {
  app.registry.add({ id: "sidebar.toggle", title: "Toggle sidebar", chord: "Meta+B", run: () => sidebarToggle.run() });
  for (const t of ["system", "light", "dark"] as const)
    app.registry.add({ id: `theme.${t}`, title: `Theme: ${t[0].toUpperCase()}${t.slice(1)}`, run: () => setTheme(t) });
}
const root = createRoot(document.getElementById("root")!);
root.render(typeof app === "string" ? <p className="p-6 text-muted-foreground">{app}</p> : <Root app={app} />);
