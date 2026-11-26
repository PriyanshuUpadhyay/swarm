import { GitBranchIcon } from "lucide-react";
import { useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { workspaceRow } from "../core/nav";
import { BranchDiff, PrChip } from "./pr-chip";

export function BranchBar({ app, workspace }: { app: App; workspace: string }) {
  const facts = useSyncExternalStore(app.subscribe, () => workspaceRow(app.view.tree, workspace));
  if (!facts?.branch) return null;
  return (
    <div className="branch-bar" data-testid="branch-bar">
      <span className="branch-name" title={facts.branch}>
        <GitBranchIcon aria-hidden />
        <span>{facts.branch}</span>
      </span>
      <PrChip pr={facts.pr} />
      <BranchDiff diff={facts.diff} />
    </div>
  );
}
