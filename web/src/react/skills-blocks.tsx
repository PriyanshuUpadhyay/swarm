// The Blocks page (ADR 0111): the run tree of a starting plan, top to bottom, beside the block library.
// The tree follows the W3C APG tree view; every fact comes from the kit through core/skills.

import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { App } from "../core/app";
import {
  BLANK,
  firstSentence,
  offeredIn,
  planOf,
  planSummary,
  stepChildren,
  stepsLoading,
  type Kit,
  type Step,
} from "../core/skills";
import { FIELD } from "./settings-shared";
import { useSkills } from "./skills";
import { Detail, Dot, kindName, Library } from "./skills-library";

/** Which occurrence is selected: a tree item by its key, or the library row. */
type Sel = { id: string; where: "tree" | "list"; key: string };

const num = (file: string) => file.split("-")[0];

function Chevron({ open }: { open: boolean }) {
  return (
    <svg aria-hidden viewBox="0 0 16 16" className={`size-3.5 shrink-0 text-muted-foreground transition-transform duration-150 ${open ? "rotate-90" : ""}`}>
      <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** Every step open, and the offered tray. */
function allOpen(kit: Kit, preset: string) {
  return new Set([...(planOf(kit, preset)?.steps ?? []).map((s) => `s:${s.file}`), "tray"]);
}

export function Blocks({ app, kit }: { app: App; kit: Kit }) {
  const preset = useSkills(app, "preset");
  const chosen = useSkills(app, "block");
  const [open, setOpen] = useState(() => allOpen(kit, preset));
  const [sel, setSel] = useState<Sel | null>(null);
  const [focus, setFocus] = useState("front");
  const tree = useRef<HTMLUListElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const byId = new Map(kit.blocks.map((b) => [b.id, b]));
  const plan = planOf(kit, preset);

  // A selection kept from an earlier open shows in the library.
  useEffect(() => {
    if (chosen && byId.has(chosen) && !sel) setSel({ id: chosen, where: "list", key: "list" });
    // Only on the first draw: later selections set both.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!sel) return;
    // Show the open detail, and the block's place on the other side.
    const here = sel.where === "list" ? list.current?.querySelector("[aria-current]") : tree.current?.querySelector('[aria-selected="true"], [data-detail]');
    const other = sel.where === "list" ? tree.current?.querySelector("[data-hit]") : list.current?.querySelector("[aria-current]");
    here?.scrollIntoView({ block: "nearest" });
    other?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const toggle = (key: string, to?: boolean) =>
    setOpen((o) => {
      const n = new Set(o);
      if (to ?? !n.has(key)) n.add(key);
      else n.delete(key);
      return n;
    });

  const select = (next: Sel) => {
    if (sel && sel.id === next.id && sel.where === next.where && sel.key === next.key) {
      setSel(null);
      app.skills.selectBlock(null);
      return;
    }
    if (next.where === "list") {
      // Open each step that loads the block, and its family node, so the reader sees where it sits.
      setOpen((o) => {
        const n = new Set(o);
        const block = byId.get(next.id)!;
        for (const s of plan?.steps ?? []) {
          if (!s.blocks.includes(next.id) && !block.parts.some((p) => s.blocks.includes(p))) continue;
          n.add(`s:${s.file}`);
          if (block.parent && s.blocks.includes(next.id)) n.add(`f:${s.file}:${block.parent}`);
        }
        if (block.offered_by.includes(preset)) n.add("tray");
        return n;
      });
    }
    setSel(next);
    app.skills.selectBlock(next.id);
  };

  const start = (value: string) => {
    app.skills.selectPreset(value);
    setOpen(allOpen(kit, value));
    setSel(null);
    setFocus("front");
  };

  const onKey = (e: KeyboardEvent<HTMLUListElement>) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>('[role="treeitem"]');
    if (!item || (e.target as HTMLElement).closest("[data-detail]")) return;
    const all = [...(tree.current?.querySelectorAll<HTMLElement>('[role="treeitem"]') ?? [])];
    const i = all.indexOf(item);
    const key = item.dataset.key!;
    const expanded = item.getAttribute("aria-expanded");
    const go = (el: HTMLElement | null | undefined) => {
      if (!el) return;
      setFocus(el.dataset.key!);
      el.focus();
    };
    switch (e.key) {
      case "ArrowDown":
        go(all[i + 1]);
        break;
      case "ArrowUp":
        go(all[i - 1]);
        break;
      case "Home":
        go(all[0]);
        break;
      case "End":
        go(all[all.length - 1]);
        break;
      case "ArrowRight":
        if (expanded === "false") toggle(key, true);
        else if (expanded === "true") go(item.querySelector<HTMLElement>('[role="treeitem"]'));
        break;
      case "ArrowLeft":
        if (expanded === "true") toggle(key, false);
        else go(item.parentElement?.closest<HTMLElement>('[role="treeitem"]'));
        break;
      case "Enter":
      case " ":
        if (item.dataset.block) select({ id: item.dataset.block, where: "tree", key });
        else if (expanded) toggle(key);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  /** Props for one tree item, with a roving tab stop. */
  const item = (key: string, level: number, extra: Record<string, unknown> = {}) => ({
    role: "treeitem",
    "aria-level": level,
    "data-key": key,
    tabIndex: key === focus ? 0 : -1,
    onFocus: (e: { target: EventTarget; currentTarget: EventTarget }) => e.target === e.currentTarget && setFocus(key),
    ...extra,
  });
  const isSel = (key: string) => sel?.where === "tree" && sel.key === key;
  const hit = (id: string) => (sel?.id === id ? { "data-hit": true } : {});
  const detail = (id: string) => <Detail kit={kit} preset={preset} block={byId.get(id)!} />;

  const card = (step: Step, id: string) => {
    const b = byId.get(id)!;
    const key = `b:${step.file}:${id}`;
    const where = stepsLoading(kit, preset, id);
    const ring = isSel(key) ? "border-ring bg-selected" : sel?.id === id ? "border-ring/60" : "border-border hover:border-border-strong";
    if (where[0] !== step.file)
      return (
        <li key={key} {...item(key, 2, { "data-block": id, "aria-selected": isSel(key) })} {...hit(id)} className={`min-w-0 rounded-lg border border-dashed px-3 py-2 outline-none focus-visible:ring-2 focus-visible:ring-ring ${ring}`} onClick={(e) => e.target === e.currentTarget || !(e.target as HTMLElement).closest("[data-detail]") ? select({ id, where: "tree", key }) : undefined}>
          <span className="flex items-center gap-2">
            <Dot kind={b.kind} />
            <code className="text-xs font-medium">{id}</code>
            <span className="text-xs text-muted-foreground">same block as in {where[0]}</span>
          </span>
          {isSel(key) && detail(id)}
        </li>
      );
    const also = where.filter((f) => f !== step.file).map(num);
    return (
      <li key={key} {...item(key, 2, { "data-block": id, "aria-selected": isSel(key) })} {...hit(id)} className={`min-w-0 rounded-lg border px-3 py-2 outline-none focus-visible:ring-2 focus-visible:ring-ring ${ring}`} onClick={(e) => (!(e.target as HTMLElement).closest("[data-detail]") ? select({ id, where: "tree", key }) : undefined)}>
        <span className="flex items-center gap-2">
          <Dot kind={b.kind} />
          <code className="text-xs font-medium">{id}</code>
          <span className="text-xs text-muted-foreground">{kindName(b.kind)}</span>
          {also.length > 0 && <span className="ml-auto rounded border border-border px-1.5 text-[11px] text-muted-foreground">also in {also.join(", ")}</span>}
        </span>
        {isSel(key) ? detail(id) : <span className="mt-1 block truncate text-xs text-muted-foreground">{firstSentence(b.description)}</span>}
      </li>
    );
  };

  const familyNode = (step: Step, family: string, parts: string[]) => {
    const f = byId.get(family);
    const key = `f:${step.file}:${family}`;
    const isOpen = open.has(key);
    const all = f?.parts ?? parts;
    return (
      <li key={key} {...item(key, 2, { "aria-expanded": isOpen, "data-family": family })} className="min-w-0 rounded-lg border border-border px-3 py-2 outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <button type="button" tabIndex={-1} className="flex w-full items-center gap-2 text-left" onClick={() => toggle(key)}>
          <Chevron open={isOpen} />
          <Dot kind="parent" />
          <code className="text-xs font-medium">{family}</code>
          <span className="text-xs text-muted-foreground">family, {all.length} parts</span>
          <span className="ml-auto rounded border border-border px-1.5 text-[11px] text-muted-foreground">
            {parts.length} of {all.length} load here
          </span>
        </button>
        <span className="mt-1 block truncate pl-5.5 text-xs text-muted-foreground">Loads {parts.join(", ")}. The other parts stay off.</span>
        {isOpen && (
          <ul role="group" className="mt-2 grid grid-cols-[minmax(0,1fr)] gap-1 border-l border-border pl-3">
            {all.map((p) => {
              const pk = `p:${step.file}:${p}`;
              const on = parts.includes(p);
              const elsewhere = stepsLoading(kit, preset, p).filter((x) => x !== step.file).map(num);
              const state = on ? `loads here${elsewhere.length ? `, also in ${elsewhere.join(", ")}` : ""}` : elsewhere.length ? `not loaded here, ${elsewhere.join(", ")} loads it` : "not loaded here";
              return (
                <li key={pk} {...item(pk, 3, { "data-block": p, "aria-selected": isSel(pk) })} {...hit(p)} className={`rounded-md px-2 py-1 outline-none focus-visible:ring-2 focus-visible:ring-ring ${isSel(pk) ? "bg-selected" : "hover:bg-hover"} ${on ? "" : "opacity-60"}`} onClick={(e) => (!(e.target as HTMLElement).closest("[data-detail]") ? select({ id: p, where: "tree", key: pk }) : undefined)}>
                  <span className="flex items-center gap-2">
                    <Dot kind={byId.get(p)?.kind ?? "rule"} />
                    <code className="text-xs">{p}</code>
                    <span className="ml-auto text-[11px] text-muted-foreground">{state}</span>
                  </span>
                  {isSel(pk) && detail(p)}
                </li>
              );
            })}
          </ul>
        )}
      </li>
    );
  };

  const stepRow = (key: string, isOpen: boolean, head: ReactNode, children: ReactNode, title?: string) => (
    <li key={key} {...item(key, 1, { "aria-expanded": isOpen })} className="relative pl-6 outline-none focus-visible:[&>button]:ring-2 focus-visible:[&>button]:ring-ring">
      <span aria-hidden className="absolute top-2.5 left-0 size-2.5 rounded-full border border-muted-foreground bg-background" />
      <button type="button" tabIndex={-1} title={title} className="flex w-full min-w-0 items-center gap-2 rounded-md px-1 py-1 text-left hover:bg-hover" onClick={() => toggle(key)}>
        <Chevron open={isOpen} />
        {head}
      </button>
      {isOpen && (
        <ul role="group" className="mt-1 mb-2 grid grid-cols-[minmax(0,1fr)] gap-1.5 pl-5">
          {children}
        </ul>
      )}
    </li>
  );

  const offered = offeredIn(kit, preset);

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-3 border-b border-border px-4 py-3">
          <label className="text-sm font-medium" htmlFor="skills-start">
            Start from
          </label>
          <select id="skills-start" className={`${FIELD} w-44 text-sm`} data-testid="skills-start" value={preset} onChange={(e) => start(e.target.value)}>
            {kit.presets.map((p) => (
              <option key={p.preset} value={p.preset}>
                {p.preset}
              </option>
            ))}
            <option value={BLANK}>Blank: front door only</option>
          </select>
          <p className="text-xs text-muted-foreground">A starting plan. The planner edits it for each task.</p>
        </div>
        <div className="flex items-center gap-3 px-4 pt-3 pb-2 text-xs text-muted-foreground">
          <p data-testid="skills-summary">{planSummary(kit, preset)}</p>
          <p className="ml-auto flex shrink-0 items-center gap-1" aria-hidden>
            <kbd className="rounded border border-border px-1">↑↓</kbd> move <kbd className="rounded border border-border px-1">→</kbd> open <kbd className="rounded border border-border px-1">←</kbd> close <kbd className="rounded border border-border px-1">↵</kbd> select
          </p>
        </div>
        <ul ref={tree} role="tree" aria-label={`Run tree, starting from ${preset === BLANK ? "a blank plan" : preset}`} className="relative min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-4 pb-4 text-sm before:absolute before:top-3 before:bottom-4 before:left-[21px] before:w-px before:bg-border" data-testid="skills-tree" onKeyDown={onKey}>
          <li {...item("front", 1)} className="relative pl-6 outline-none focus-visible:ring-2 focus-visible:ring-ring" data-testid="skills-front">
            <span aria-hidden className="absolute top-2.5 left-0 size-2.5 rounded-full bg-muted-foreground" />
            <span className="flex flex-wrap items-center gap-1.5 px-1 py-1">
              <span className="mr-1 font-medium">Front door</span>
              {kit.front.map((id, i) => (
                <span key={id} className="flex items-center gap-1.5">
                  {i > 0 && <span aria-hidden className="text-muted-foreground">→</span>}
                  <button type="button" tabIndex={-1} data-block={id} {...hit(id)} className={`rounded border px-1.5 py-0.5 font-mono text-xs ${sel?.id === id ? "border-ring bg-selected" : "border-border-strong hover:border-ring"}`} onClick={() => select({ id, where: "tree", key: "front" })}>
                    {id}
                  </button>
                </span>
              ))}
              <span className="ml-1 text-xs text-muted-foreground">runs first for every task</span>
            </span>
            {isSel("front") && sel && detail(sel.id)}
          </li>
          {(plan?.steps ?? []).map((s) =>
            stepRow(
              `s:${s.file}`,
              open.has(`s:${s.file}`),
              <>
                <code className="shrink-0 text-xs font-semibold">{s.file.replace(/\.md$/, "")}</code>
                <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{s.holds.replace(/`/g, "")}</span>
                <span className="shrink-0 text-[11px] text-muted-foreground">
                  {s.blocks.length} block{s.blocks.length === 1 ? "" : "s"}
                </span>
              </>,
              stepChildren(kit, s).map((c) => ("family" in c ? familyNode(s, c.family, c.parts) : card(s, c.block))),
              s.why,
            ),
          )}
          {plan ? (
            stepRow(
              "tray",
              open.has("tray"),
              <>
                <span className="shrink-0 text-xs font-semibold">Offered</span>
                <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">off until the planner adds it</span>
                <span className="shrink-0 text-[11px] text-muted-foreground">{offered.length} blocks</span>
              </>,
              <li className="flex flex-wrap gap-1.5" data-testid="skills-offered">
                {offered.map((b) => {
                  const key = `o:${b.id}`;
                  return (
                    <span key={key} {...item(key, 2, { "data-block": b.id, "aria-selected": isSel(key) })} {...hit(b.id)} className={`flex cursor-pointer items-center gap-1.5 rounded-md border border-dashed px-2 py-1 outline-none focus-visible:ring-2 focus-visible:ring-ring ${isSel(key) ? "border-ring bg-selected" : "border-border-strong hover:border-ring"}`} onClick={() => select({ id: b.id, where: "tree", key })}>
                      <Dot kind={b.kind} />
                      <code className="text-xs">{b.id}</code>
                    </span>
                  );
                })}
                {sel?.where === "tree" && sel.key.startsWith("o:") && <div className="w-full rounded-lg border border-border px-3 py-2">{detail(sel.id)}</div>}
              </li>,
            )
          ) : (
            <li className="relative pl-6 text-xs text-muted-foreground">
              <span aria-hidden className="absolute top-1 left-0 size-2.5 rounded-full border border-muted-foreground bg-background" />
              No steps yet. The planner writes them from the task.
            </li>
          )}
        </ul>
      </div>
      <Library app={app} kit={kit} preset={preset} selected={sel?.id ?? null} expanded={sel?.where === "list"} listRef={list} onPick={(id) => select({ id, where: "list", key: "list" })} detail={detail} />
    </div>
  );
}
