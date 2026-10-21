// The Skills dialog (ADR 0111). The shell is small; the pages and the graph layout load on the
// first open, so the chat app does not pay for them.

import { lazy, Suspense, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import type { SkillsState, SkillsView } from "../core/skills";
import { Dialog, DialogContent, DialogTitle } from "../components/ui/dialog";

const SkillsPage = lazy(() => import("./skills-page").then((m) => ({ default: m.SkillsPage })));

const VIEWS: [SkillsView, string][] = [
  ["catalog", "Catalog"],
  ["graph", "Skill graph"],
  ["composer", "Composer"],
];

export function useSkills<K extends keyof SkillsState>(app: App, key: K): SkillsState[K] {
  return useSyncExternalStore(app.skills.subscribe, () => app.skills.state[key]);
}

export function SkillsDialog({ app }: { app: App }) {
  const open = useSkills(app, "open");
  const view = useSkills(app, "view");
  return (
    <Dialog open={open} onOpenChange={(o) => !o && app.skills.close()}>
      {open && (
        <DialogContent className="flex h-[min(760px,calc(100vh-104px))] w-[min(1180px,calc(100vw-32px))] p-0" data-testid="skills" aria-describedby={undefined}>
          <nav aria-label="Skills pages" className="flex w-40 shrink-0 flex-col gap-0.5 border-r border-border p-2">
            <DialogTitle className="px-2 pt-1 pb-2">Skills</DialogTitle>
            {VIEWS.map(([id, title]) => (
              <button
                key={id}
                className={`rounded-md px-2 py-1.5 text-left text-sm ${view === id ? "bg-selected text-foreground" : "text-muted-foreground hover:bg-hover hover:text-foreground"}`}
                aria-current={view === id ? "page" : undefined}
                data-testid={`skills-view-${id}`}
                onClick={() => app.skills.setView(id)}
              >
                {title}
              </button>
            ))}
          </nav>
          <div className="flex min-w-0 flex-1 flex-col" data-testid={`skills-${view}`}>
            <Suspense fallback={<p role="status" className="p-5 text-sm text-muted-foreground">Loading…</p>}>
              <SkillsPage app={app} view={view} />
            </Suspense>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
