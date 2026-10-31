// The block library of the Blocks page (ADR 0111): every block in one searchable list, and the
// detail that both the list and the run tree open.

import type { ReactNode, RefObject } from "react";
import type { App } from "../core/app";
import { firstSentence, libraryRows, libraryTag, loadedBy, whereInPlan, type Block, type Kit } from "../core/skills";
import { FIELD } from "./settings-shared";
import { useSkills } from "./skills";

const DOT: Record<string, string> = { rule: "bg-sky-500", step: "bg-emerald-500", pack: "bg-orange-500", parent: "bg-pink-500" };
export const kindName = (k: string) => (k === "parent" ? "family" : k);

export function Dot({ kind }: { kind: string }) {
  return <span aria-hidden className={`size-2 shrink-0 rounded-full ${DOT[kind] ?? "bg-muted-foreground"}`} />;
}

type LibraryProps = {
  app: App;
  kit: Kit;
  preset: string;
  selected: string | null;
  /** True when the selection was made in the list, so the list shows its detail. */
  expanded: boolean;
  listRef: RefObject<HTMLUListElement | null>;
  onPick: (id: string) => void;
  detail: (id: string) => ReactNode;
};

export function Library({ app, kit, preset, selected, expanded, listRef, onPick, detail }: LibraryProps) {
  const query = useSkills(app, "filter");
  const rows = libraryRows(kit, query);
  return (
    <aside aria-label="Block library" className="flex w-80 shrink-0 flex-col border-l border-border">
      <div className="grid gap-2 p-3">
        <div className="flex items-baseline justify-between">
          <h3 className="text-sm font-medium">Block library</h3>
          <span className="text-xs text-muted-foreground" data-testid="skills-count">
            {query.trim() ? `${rows.length} of ${kit.blocks.length}` : kit.blocks.length}
          </span>
        </div>
        <input
          className={`${FIELD} w-full text-sm`}
          type="search"
          aria-label="Filter blocks"
          placeholder="Which block does…"
          autoComplete="off"
          spellCheck={false}
          data-testid="skills-filter"
          autoFocus
          value={query}
          onChange={(e) => app.skills.setFilter(e.target.value)}
        />
        <p className="flex gap-3 text-[11px] text-muted-foreground">
          {(["rule", "step", "pack", "parent"] as const).map((k) => (
            <span key={k} className="flex items-center gap-1">
              <Dot kind={k} />
              {kindName(k)}
            </span>
          ))}
        </p>
      </div>
      <ul ref={listRef} className="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="skills-library">
        {rows.length === 0 && <li className="px-2 text-sm text-muted-foreground">No block matches. Try a word from what it does.</li>}
        {rows.map(({ block: b, part }) => {
          const current = selected === b.id;
          const tag = libraryTag(kit, preset, b.id);
          return (
            <li key={b.id} aria-current={current || undefined}>
              <button
                type="button"
                data-block={b.id}
                aria-expanded={current && expanded}
                className={`flex w-full items-center gap-2 rounded-md py-1 pr-2 text-left ${part ? "pl-5" : "pl-2"} ${current ? "bg-selected" : "hover:bg-hover"}`}
                onClick={() => onPick(b.id)}
              >
                <Dot kind={b.kind} />
                <code className={`shrink-0 text-xs ${tag ? "font-semibold" : ""}`}>{b.id}</code>
                <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">{firstSentence(b.description)}</span>
                {tag && <span className="shrink-0 font-mono text-[10px] text-muted-foreground">{tag}</span>}
              </button>
              {current && expanded && <div className="px-2 pb-2">{detail(b.id)}</div>}
            </li>
          );
        })}
      </ul>
    </aside>  );
}

export function Detail({ kit, preset, block }: { kit: Kit; preset: string; block: Block }) {
  const loaded = loadedBy(block);
  return (
    <div data-detail className="mt-2 grid cursor-auto gap-2 text-xs" data-testid="skills-detail" onClick={(e) => e.stopPropagation()}>
      <p className="text-sm leading-relaxed">{block.description}</p>
      <div className="grid grid-cols-[1fr_auto_1fr] items-stretch gap-2" data-testid="skills-io">
        <div className="rounded-md border border-border bg-background px-2.5 py-1.5">
          <span className="block text-[11px] text-muted-foreground">Needs</span>
          {block.needs || "—"}
        </div>
        <span aria-label="gives" className="self-center text-muted-foreground">
          →
        </span>
        <div className="rounded-md border border-border bg-background px-2.5 py-1.5">
          <span className="block text-[11px] text-muted-foreground">Gives</span>
          {block.gives || "—"}
        </div>
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <dt className="text-muted-foreground">This plan</dt>
        <dd data-testid="skills-where">{whereInPlan(kit, preset, block)}</dd>
        <dt className="text-muted-foreground">Loaded by</dt>
        <dd data-testid="skills-used-by">{loaded.length ? loaded.map(([p, steps]) => `${p} ${steps.join(", ")}`).join("; ") : kit.front.includes(block.id) ? "every task" : "no preset"}</dd>
        <dt className="text-muted-foreground">Offered by</dt>
        <dd data-testid="skills-offered-by">{block.offered_by.length ? block.offered_by.join(", ") : "no preset"}</dd>
        {block.kind === "parent" && (
          <>
            <dt className="text-muted-foreground">Parts</dt>
            <dd>{block.parts.join(", ")}. A step loads only the parts it names.</dd>
          </>
        )}
        {block.parent && (
          <>
            <dt className="text-muted-foreground">Family</dt>
            <dd>{block.parent}. Loading this part does not load the others.</dd>
          </>
        )}
      </dl>
    </div>
  );
}
