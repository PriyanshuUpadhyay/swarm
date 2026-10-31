// The Skills pages, loaded on the first open (ADR 0111). They show what `preset.py` says and work
// out no fact of their own.

import type { App } from "../core/app";
import type { SkillsView } from "../core/skills";
import { Button } from "../components/ui/button";
import { Blocks } from "./skills-blocks";
import { Composer } from "./skills-composer";
import { useSkills } from "./skills";

export function SkillsPage({ app, view }: { app: App; view: SkillsView }) {
  const kit = useSkills(app, "kit");
  if (kit.state === "error")
    return (
      <div role="alert" className="grid content-start gap-3 p-5 text-sm">
        <p className="font-medium">The kit could not be read.</p>
        <p className="font-mono text-xs break-words whitespace-pre-wrap text-muted-foreground">{kit.error}</p>
        {/* load() keeps its own failure in state.kit. */}
        <Button variant="outline" size="sm" className="w-fit" onClick={() => void app.skills.load()}>
          Try again
        </Button>
      </div>
    );
  if (kit.state !== "ready") return <p role="status" className="p-5 text-sm text-muted-foreground">Reading the kit…</p>;
  switch (view) {
    case "catalog":
      return <Blocks app={app} kit={kit.data} />;
    case "composer":
      return <Composer app={app} kit={kit.data} />;
    default:
      return view satisfies never;
  }
}
