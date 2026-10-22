// The Skills pages, loaded on the first open (ADR 0111). They show what `preset.py` says and work
// out no fact of their own.

import { useState } from "react";
import type { App } from "../core/app";
import { blockTag, catalogGroups, usedByLabel, type Block, type Kit, type Preset, type SkillsView } from "../core/skills";
import { Button } from "../components/ui/button";
import { FIELD } from "./settings-shared";
import { SkillGraph } from "./skill-graph";
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
      return <Catalog app={app} kit={kit.data} />;
    case "graph":
      return <Graphs app={app} kit={kit.data} />;
    case "composer":
      return <Composer app={app} kit={kit.data} />;
    default:
      return view satisfies never;
  }
}

// ---- Catalog ----

function Catalog({ app, kit }: { app: App; kit: Kit }) {
  const [filter, setFilter] = useState("");
  const chosen = useSkills(app, "block");
  const groups = catalogGroups(kit, filter);
  const block = kit.blocks.find((b) => b.id === chosen) ?? null;
  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex w-72 shrink-0 flex-col border-r border-border">
        <div className="p-3 pb-2">
          <input
            className={`${FIELD} w-full text-sm`}
            type="search"
            aria-label="Filter blocks"
            placeholder="Filter blocks…"
            autoComplete="off"
            spellCheck={false}
            data-testid="skills-filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <p className="mt-2 text-xs text-muted-foreground">
            {kit.blocks.length} blocks, {kit.presets.length} default skills · kit {kit.version}
          </p>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="skills-tree">
          {groups.length === 0 && <p role="status" className="px-2 text-sm text-muted-foreground">No block matches.</p>}
          {groups.map((g) => (
            <section key={g.title} aria-label={g.title} className="mt-2">
              <h3 className="px-2 pb-1 text-xs font-medium text-muted-foreground">{g.title}</h3>
              {g.items.map(({ block: b, depth }) => (
                <button
                  key={b.id}
                  className={`flex w-full items-center gap-2 rounded-md py-1 pr-2 text-left font-mono text-xs ${depth ? "pl-6" : "pl-2"} ${b.id === chosen ? "bg-selected text-foreground" : "text-foreground hover:bg-hover"}`}
                  aria-current={b.id === chosen || undefined}
                  data-block={b.id}
                  onClick={() => app.skills.selectBlock(b.id)}
                >
                  <span className="min-w-0 flex-1 truncate">{b.id}</span>
                  {blockTag(kit, b) && <span className="font-sans text-[11px] text-muted-foreground">{blockTag(kit, b)}</span>}
                </button>
              ))}
            </section>
          ))}
        </div>
      </div>
      <div className="min-w-0 flex-1 overflow-y-auto p-5" data-testid="skills-detail">
        {block ? <BlockDetail app={app} kit={kit} block={block} /> : <p className="text-sm text-muted-foreground">Pick a block to see what it needs, what it gives, and which skills use it.</p>}
      </div>
    </div>
  );
}

function BlockDetail({ app, kit, block }: { app: App; kit: Kit; block: Block }) {
  const graph = (preset: string) => {
    app.skills.selectPreset(preset);
    app.skills.setView("graph");
  };
  return (
    <div className="grid max-w-2xl gap-4 text-sm">
      <div>
        <h3 className="font-mono text-base font-semibold">{block.id}</h3>
        <p className="text-xs text-muted-foreground">
          {block.kind}
          {block.parent && ` · part of ${block.parent}`}
        </p>
      </div>
      <p>{block.description}</p>
      <Fact name="Needs" text={block.needs} />
      <Fact name="Gives" text={block.gives} />
      {block.parts.length > 0 && <Fact name="Parts" text={block.parts.join(", ")} mono />}
      <div>
        <h4 className="text-xs font-medium text-muted-foreground">Used by</h4>
        {block.used_by.length === 0 ? (
          <p className="mt-1" data-testid="skills-unused">
            {kit.front.includes(block.id) ? "the plan front door, before step 1 of every skill" : "not used by a default preset"}
          </p>
        ) : (
          <ul className="mt-1 grid" data-testid="skills-used-by">
            {block.used_by.map((u) => (
              <li key={`${u.preset}/${u.step}`}>
                <button className="py-1 font-mono text-xs underline-offset-2 hover:underline" onClick={() => graph(u.preset)}>
                  {u.preset} · {u.step}
                </button>
                <span className="ml-2 text-xs text-muted-foreground">{usedByLabel(block, u)}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
      {block.offered_by.length > 0 && (
        <div>
          <h4 className="text-xs font-medium text-muted-foreground">Offered by (off until the planner adds it)</h4>
          <ul className="mt-1 grid" data-testid="skills-offered-by">
            {block.offered_by.map((p) => (
              <li key={p}>
                <button className="py-1 font-mono text-xs underline-offset-2 hover:underline" onClick={() => graph(p)}>
                  {p}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function Fact({ name, text, mono }: { name: string; text: string; mono?: boolean }) {
  return (
    <div>
      <h4 className="text-xs font-medium text-muted-foreground">{name}</h4>
      <p className={`mt-1 ${mono ? "font-mono text-xs" : ""}`}>{text || "—"}</p>
    </div>
  );
}

// ---- Skill graph ----

function Graphs({ app, kit }: { app: App; kit: Kit }) {
  const chosen = useSkills(app, "preset");
  const preset = kit.presets.find((p) => p.preset === chosen) ?? kit.presets[0];
  const [step, setStep] = useState<string | null>(null);
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <nav aria-label="Default skills" className="flex flex-wrap gap-1 border-b border-border p-3">
        {kit.presets.map((p) => (
          <button
            key={p.preset}
            aria-current={p.preset === preset?.preset || undefined}
            className={`rounded-md px-2 py-1 font-mono text-xs ${p.preset === preset?.preset ? "bg-selected text-foreground" : "text-muted-foreground hover:bg-hover hover:text-foreground"}`}
            data-preset={p.preset}
            onClick={() => {
              app.skills.selectPreset(p.preset);
              setStep(null);
            }}
          >
            {p.preset}
          </button>
        ))}
      </nav>
      {preset && <PresetView key={preset.preset} app={app} kit={kit} preset={preset} step={step} onStep={setStep} />}
    </div>
  );
}

function PresetView({ app, kit, preset, step, onStep }: { app: App; kit: Kit; preset: Preset; step: string | null; onStep: (file: string) => void }) {
  const chosen = preset.steps.find((s) => s.file === step) ?? null;
  const duty = new Set(kit.blocks.filter((b) => b.duty).map((b) => b.id));
  const open = (block: string) => {
    app.skills.selectBlock(block);
    app.skills.setView("catalog");
  };
  return (
    <div className="flex min-h-0 flex-1">
      <div className="min-w-0 flex-1 overflow-auto p-4">
        <p className="mb-3 text-xs text-muted-foreground">
          {preset.steps.length} steps · stops for {preset.stop === "owner" ? "the owner" : "the chat"}
          {preset.mode && ` · ${preset.mode}`}
        </p>
        {preset.start && (
          <p className="mb-3 text-xs text-muted-foreground" data-testid="skills-view-only">
            View only: this skill starts its own run.
          </p>
        )}
        <div className="mb-3 flex flex-wrap items-center gap-1 text-xs text-muted-foreground" data-testid="skills-front">
          <span className="mr-1">Plan first:</span>
          {kit.front.map((b, i) => (
            <span key={b} className="flex items-center gap-1">
              {i > 0 && <span aria-hidden>→</span>}
              <button className="rounded border border-border-strong px-1.5 py-0.5 font-mono text-foreground hover:border-ring" data-block={b} onClick={() => open(b)}>
                {b}
              </button>
            </span>
          ))}
          <span aria-hidden>→ step 1</span>
        </div>
        <SkillGraph steps={preset.steps} duty={duty} selected={step} onSelect={onStep} />
        {preset.offers.length > 0 && (
          <section aria-label="Offered blocks" className="mt-4 rounded-lg border border-dashed border-border p-3 text-xs" data-testid="skills-offered">
            <h3 className="mb-2 font-medium text-muted-foreground">Offered · off by default; the planner adds one when the task needs it</h3>
            <div className="flex flex-wrap gap-1">
              {preset.offers.map((b) => (
                <button key={b} className="rounded border border-dashed border-border-strong px-1.5 py-0.5 font-mono text-muted-foreground hover:border-ring hover:text-foreground" data-block={b} onClick={() => open(b)}>
                  {b}
                </button>
              ))}
            </div>
          </section>
        )}
      </div>
      {chosen && (
        <aside className="grid w-72 shrink-0 content-start gap-3 overflow-y-auto border-l border-border p-4 text-sm" data-testid="skills-step">
          <h3 className="font-mono font-semibold">{chosen.file}</h3>
          <Fact name="Needs" text={chosen.needs} />
          <Fact name="Holds" text={chosen.holds} />
          <Fact name="Why" text={chosen.why} />
          <Fact name="Loads" text={chosen.resolved.join(", ")} mono />
        </aside>
      )}
    </div>
  );
}
