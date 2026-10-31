// The Skills pages, loaded on the first open (ADR 0111). They show what `preset.py` says and work
// out no fact of their own.

import type { App } from "../core/app";
import { blockTag, catalogGroups, usedByLabel, usedIn, type Block, type Kit, type SkillsView } from "../core/skills";
import { Button } from "../components/ui/button";
import { FIELD } from "./settings-shared";
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
    case "composer":
      return <Composer app={app} kit={kit.data} />;
    default:
      return view satisfies never;
  }
}

// ---- Catalog ----

function Kind({ kind }: { kind: string }) {
  return <span className="rounded border border-border-strong px-1 py-px font-sans text-[10px] leading-none text-muted-foreground">{kind}</span>;
}

function Catalog({ app, kit }: { app: App; kit: Kit }) {
  const filter = useSkills(app, "filter");
  const chosen = useSkills(app, "block");
  const preset = useSkills(app, "preset");
  const groups = catalogGroups(kit, filter, preset);
  const block = kit.blocks.find((b) => b.id === chosen) ?? null;
  const shown = groups.reduce((n, g) => n + g.items.length + (g.parent ? 1 : 0), 0);
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border p-3">
        <input
          className={`${FIELD} w-56 text-sm`}
          type="search"
          aria-label="Filter blocks"
          placeholder="Filter blocks…"
          autoComplete="off"
          spellCheck={false}
          data-testid="skills-filter"
          autoFocus
          value={filter}
          onChange={(e) => app.skills.setFilter(e.target.value)}
        />
        <nav aria-label="Presets" className="flex flex-wrap gap-1" data-testid="skills-presets">
          {[null, ...kit.presets.map((p) => p.preset)].map((p) => (
            <button
              key={p ?? "all"}
              aria-pressed={preset === p}
              data-preset={p ?? "all"}
              className={`rounded-full border px-2.5 py-0.5 font-mono text-xs ${preset === p ? "border-ring bg-selected text-foreground" : "border-border text-muted-foreground hover:bg-hover hover:text-foreground"}`}
              onClick={() => app.skills.selectPreset(p)}
            >
              {p ?? "all"}
            </button>
          ))}
        </nav>
        <p className="ml-auto text-xs text-muted-foreground" data-testid="skills-count">
          {shown} of {kit.blocks.length} blocks · kit {kit.version.split("+")[0]}
        </p>
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="min-w-0 flex-1 overflow-y-auto p-4" data-testid="skills-tree">
          <p className="mb-4 flex flex-wrap items-center gap-1 text-xs text-muted-foreground" data-testid="skills-front">
            <span className="mr-1">Every task starts with</span>
            {kit.front.map((b, i) => (
              <span key={b} className="flex items-center gap-1">
                {i > 0 && <span aria-hidden>→</span>}
                <button className="rounded border border-border-strong px-1.5 py-0.5 font-mono text-foreground hover:border-ring" data-block={b} onClick={() => app.skills.selectBlock(b)}>
                  {b}
                </button>
              </span>
            ))}
            <span className="ml-1">first. They pick the blocks below for the task.</span>
          </p>
          {groups.length === 0 && <p role="status" className="text-sm text-muted-foreground">No block matches.</p>}
          {groups.map((g) => (
            <section key={g.title} aria-label={g.title} className="mb-5">
              {g.parent ? (
                <button
                  className={`mb-2 flex w-full items-baseline gap-2 rounded-md px-1 text-left ${g.parent.id === chosen ? "bg-selected" : "hover:bg-hover"}`}
                  aria-current={g.parent.id === chosen || undefined}
                  data-block={g.parent.id}
                  onClick={() => app.skills.selectBlock(g.parent!.id)}
                >
                  <h3 className="font-mono text-xs font-semibold">{g.parent.id}</h3>
                  <span className="text-[11px] text-muted-foreground">family · {g.parent.parts.length} parts</span>
                </button>
              ) : (
                <h3 className="mb-2 px-1 text-xs font-medium text-muted-foreground">{g.title}</h3>
              )}
              <div className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-2">
                {g.items.map((b) => (
                  <Card key={b.id} kit={kit} block={b} chosen={b.id === chosen} onClick={() => app.skills.selectBlock(b.id)} />
                ))}
              </div>
            </section>
          ))}
        </div>
        <aside className="w-80 shrink-0 overflow-y-auto border-l border-border p-4" data-testid="skills-detail">
          {block ? (
            <BlockDetail app={app} kit={kit} block={block} />
          ) : (
            <p className="text-sm text-muted-foreground">Pick a block to see what it needs, what it gives, and which presets load it.</p>
          )}
        </aside>
      </div>
    </div>
  );
}

function Card({ kit, block, chosen, onClick }: { kit: Kit; block: Block; chosen: boolean; onClick: () => void }) {
  const tag = blockTag(kit, block);
  const used = usedIn(kit, block);
  return (
    <button
      className={`flex min-h-[92px] flex-col gap-1.5 rounded-lg border p-2.5 text-left ${chosen ? "border-ring bg-selected" : "border-border hover:border-border-strong hover:bg-hover"} ${tag === "unused" ? "opacity-60" : ""}`}
      aria-current={chosen || undefined}
      data-block={block.id}
      onClick={onClick}
    >
      <span className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-xs font-medium">{block.id}</span>
        <Kind kind={block.kind} />
      </span>
      <span className="line-clamp-2 text-[11px] leading-snug text-muted-foreground">{block.description}</span>
      <span className="mt-auto truncate font-mono text-[10px] text-muted-foreground">{tag ?? used.join(" · ")}</span>
    </button>
  );
}

function BlockDetail({ app, kit, block }: { app: App; kit: Kit; block: Block }) {
  const byPreset = kit.presets.map((p) => ({ preset: p.preset, uses: block.used_by.filter((u) => u.preset === p.preset) })).filter((x) => x.uses.length);
  const chip = "rounded border border-border-strong px-1.5 py-0.5 font-mono text-xs hover:border-ring";
  return (
    <div className="grid gap-4 text-sm">
      <div>
        <div className="flex items-center gap-2">
          <h3 className="min-w-0 flex-1 truncate font-mono text-base font-semibold">{block.id}</h3>
          <Kind kind={block.kind} />
        </div>
        {block.parent && (
          <p className="mt-0.5 text-xs text-muted-foreground">
            part of{" "}
            <button className="font-mono underline-offset-2 hover:underline" onClick={() => app.skills.selectBlock(block.parent!)}>
              {block.parent}
            </button>
          </p>
        )}
      </div>
      <p className="leading-relaxed">{block.description}</p>
      <div className="grid gap-1.5" data-testid="skills-io">
        <Fact name="Needs" text={block.needs} />
        <span aria-hidden className="pl-3 text-muted-foreground">↓</span>
        <Fact name="Gives" text={block.gives} />
      </div>
      {block.parts.length > 0 && (
        <div>
          <h4 className="text-xs font-medium text-muted-foreground">Parts</h4>
          <div className="mt-1 flex flex-wrap gap-1">
            {block.parts.map((p) => (
              <button key={p} className={chip} data-block={p} onClick={() => app.skills.selectBlock(p)}>
                {p}
              </button>
            ))}
          </div>
        </div>
      )}
      <div>
        <h4 className="text-xs font-medium text-muted-foreground">Loaded by</h4>
        {byPreset.length === 0 ? (
          <p className="mt-1 text-xs" data-testid="skills-unused">
            {kit.front.includes(block.id) ? "the plan front door, before step 1 of every task" : "no preset step; the planner can still pick it"}
          </p>
        ) : (
          <ul className="mt-1 grid gap-1.5" data-testid="skills-used-by">
            {byPreset.map(({ preset, uses }) => (
              <li key={preset} className="grid gap-0.5">
                <button className="w-fit font-mono text-xs font-medium underline-offset-2 hover:underline" onClick={() => app.skills.selectPreset(preset)}>
                  {preset}
                </button>
                {uses.map((u) => (
                  <span key={u.step} className="pl-3 font-mono text-[11px] text-muted-foreground">
                    {u.step} · {usedByLabel(block, u)}
                  </span>
                ))}
              </li>
            ))}
          </ul>
        )}
      </div>
      {block.offered_by.length > 0 && (
        <div>
          <h4 className="text-xs font-medium text-muted-foreground">Offered by · off until the planner adds it</h4>
          <div className="mt-1 flex flex-wrap gap-1" data-testid="skills-offered-by">
            {block.offered_by.map((p) => (
              <button key={p} className={`${chip} border-dashed`} onClick={() => app.skills.selectPreset(p)}>
                {p}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function Fact({ name, text }: { name: string; text: string }) {
  return (
    <div className="rounded-md border border-border bg-hover/40 px-2.5 py-2">
      <h4 className="text-[11px] font-medium text-muted-foreground">{name}</h4>
      <p className="mt-0.5 text-xs leading-relaxed">{text || "—"}</p>
    </div>
  );
}
