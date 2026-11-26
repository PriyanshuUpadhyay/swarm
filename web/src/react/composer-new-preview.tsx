// The new chat composer's card: what ⌘Enter starts (ADR 0149).

import { useEffect, useState } from "react";
import type { App } from "../core/app";
import { placeLabel, type Place, type Preview, type Target } from "../core/new-chat";

/** `/api/chat/new-preview`, read for a new worktree only. The branch is not in it: the start
 *  names it from the prompt with the on-device model, which takes seconds (ADR 0158). */
export function usePreview(app: App, target: Target, wanted: boolean): Preview | "failed" | null {
  const [preview, setPreview] = useState<Preview | "failed" | null>(null);

  useEffect(() => {
    if (!wanted) return setPreview(null);
    let live = true;
    app.newChat.preview(target.cwd).then(
      (p) => live && setPreview(p),
      () => live && setPreview("failed"), // The card is a hint; the start reads each of these again.
    );
    return () => {
      live = false;
    };
  }, [app, target.cwd, wanted]);

  return wanted ? preview : null;
}

export function PreviewCard({ target, place, worktree, preview }: { target: Target; place: Place; worktree: boolean; preview: Preview | "failed" | null }) {
  const read = (p: Preview): [string, string][] => [
    ["Branch", "named from the first prompt"],
    ["Folder", "named from the branch"],
    ["Base", p.base ?? "none"],
    ["Fetched", p.fetchedUnixS ? new Date(p.fetchedUnixS * 1000).toLocaleString() : "never"],
    ...(p.setup ? ([["Setup", p.setup]] as [string, string][]) : []),
  ];

  const elsewhere: [string, string][] = place.kind === "here" || !target.git ? [["Folder", target.cwd]] : [["Starts on", placeLabel(place, target)]];
  const waiting: [string, string][] = [["Branch", preview === "failed" ? "Not known yet; the send names it" : "Reading…"]];
  const rows = !worktree ? elsewhere : preview && preview !== "failed" ? read(preview) : waiting;

  return (
    <aside className="grid content-start gap-1.5 rounded-2xl border border-border p-3 text-sm" aria-label="What starts" data-testid="new-chat-preview">
      <h3 className="text-xs font-medium text-muted-foreground">{worktree ? "A new worktree" : "Where the chat runs"}</h3>
      {rows.map(([k, v]) => (
        <div key={k} className="grid grid-cols-[72px_1fr] gap-2">
          <span className="text-muted-foreground">{k}</span>
          <span className="truncate font-mono text-xs leading-5" data-testid={`preview-${k.toLowerCase().replace(" ", "-")}`} title={v}>
            {v}
          </span>
        </div>
      ))}
    </aside>
  );
}
