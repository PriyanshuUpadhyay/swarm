// One skill as a step graph. Each card is drawn once out of view to measure it, then the layout
// places the cards at their measured sizes, so a long block list cannot run into an edge.

import { useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { layoutSkill, type Size } from "../core/skill-layout";
import type { Step } from "../core/skills";

const PAD = 12;
const CARD = "w-[220px] rounded-lg border bg-background p-2.5 text-left text-xs shadow-xs";

/** A step with only the fields the graph draws, so the Composer can pass rows that Python checked. */
export type GraphStep = Pick<Step, "file" | "holds" | "resolved" | "blocks" | "uses" | "stage">;

export function SkillGraph({ steps, selected, onSelect }: { steps: GraphStep[]; selected?: string | null; onSelect?: (file: string) => void }) {
  const measure = useRef<HTMLDivElement>(null);
  const arrow = useId();
  const [sizes, setSizes] = useState<Size[] | null>(null);
  useLayoutEffect(() => {
    const cards = [...(measure.current?.children ?? [])] as HTMLElement[];
    setSizes(cards.map((c) => ({ w: c.offsetWidth, h: c.offsetHeight })));
  }, [steps]);
  const laid = useMemo(() => (sizes && sizes.length === steps.length ? layoutSkill(steps, sizes) : null), [steps, sizes]);
  return (
    <div className="relative" data-testid="skill-graph" style={laid ? { width: laid.width + 2 * PAD, height: laid.height + 2 * PAD } : undefined}>
      {/* h-0 keeps the measure copy out of the scroll area; its cards keep their own size. */}
      <div ref={measure} aria-hidden className="invisible absolute h-0 overflow-hidden">
        {steps.map((s) => (
          <Card key={s.file} step={s} />
        ))}
      </div>
      {laid && (
        <>
          <svg className="absolute inset-0 text-muted-foreground" width={laid.width + 2 * PAD} height={laid.height + 2 * PAD} aria-hidden>
            <defs>
              <marker id={arrow} viewBox="0 0 8 8" refX="8" refY="4" markerWidth="8" markerHeight="8" orient="auto">
                <path d="M0 0L8 4L0 8z" fill="currentColor" />
              </marker>
            </defs>
            {laid.edges.map((e) => (
              <polyline
                key={`${e.from}>${e.to}`}
                data-edge={`${e.from}>${e.to}`}
                points={e.points.map((p) => `${p.x + PAD},${p.y + PAD}`).join(" ")}
                fill="none"
                stroke="currentColor"
                strokeWidth="1.25"
                markerEnd={`url(#${CSS.escape(arrow)})`}
              />
            ))}
          </svg>
          {laid.boxes.map((b, i) => (
            <Card key={b.file} step={steps[i]} at={{ left: b.x + PAD, top: b.y + PAD }} selected={selected === b.file} onSelect={onSelect} />
          ))}
        </>
      )}
    </div>
  );
}

function Card({ step, at, selected, onSelect }: { step: GraphStep; at?: { left: number; top: number }; selected?: boolean; onSelect?: (file: string) => void }) {
  const body = (
    <>
      <span className="block font-mono font-medium [overflow-wrap:anywhere] text-foreground">{step.file}</span>
      <span className="mt-1 block [overflow-wrap:anywhere] text-muted-foreground">{step.holds}</span>
      <span className="mt-1.5 flex flex-wrap gap-1">
        {step.blocks.map((b, i) => (
          <span key={i} className="rounded bg-code px-1 font-mono text-[11px] text-foreground">
            {b}
          </span>
        ))}
      </span>
    </>
  );
  if (!at) return <div className={`${CARD} border-border`}>{body}</div>;
  if (!onSelect)
    return (
      <div className={`${CARD} absolute border-border-strong`} style={at} data-step={step.file}>
        {body}
      </div>
    );
  return (
    <button
      type="button"
      className={`${CARD} absolute ${selected ? "border-ring ring-1 ring-ring" : "border-border-strong hover:border-ring"} focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none`}
      style={at}
      data-step={step.file}
      aria-pressed={selected}
      onClick={() => onSelect(step.file)}
    >
      {body}
    </button>
  );
}
