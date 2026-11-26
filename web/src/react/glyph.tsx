import type { CSSProperties } from "react";
// The glyph: a hexagon of seven dots (DESIGN.md). At rest it is the app mark with every dot lit. While an
// agent works, a wave of lit dots runs round the ring; `prefers-reduced-motion` shows its still form, three
// lit dots. When the state is unknown, every dot is unlit. The colours and the loop time are tokens
// (`--glyph-lit`, `--glyph-unlit`, `--glyph-loop`), and styles-glyph.css owns the motion.

export type GlyphForm = "rest" | "working" | "unlit";

const LABEL: Record<GlyphForm, string> = { rest: "Swarm", working: "Working", unlit: "State unknown" };

// Centre first, then six dots round it, clockwise from the top, in a 16 x 16 box.
const DOTS = [
  [8, 8],
  ...Array.from({ length: 6 }, (_, i) => [8 + 5 * Math.sin((i * Math.PI) / 3), 8 - 5 * Math.cos((i * Math.PI) / 3)]),
] as const;

/** The lit dots of a form. The wave of "working" moves the lit ring dots; its still form keeps three. */
function lit(form: GlyphForm, dot: number) {
  if (form === "rest") return true;
  if (form === "unlit") return false;
  return dot > 0 && dot % 2 === 1;
}

export function Glyph({ form = "rest", size = 16, label }: { form?: GlyphForm; size?: 16 | 12; label?: string }) {
  return (
    <svg className="swarm-glyph" data-form={form} width={size} height={size} viewBox="0 0 16 16" role="img" aria-label={label ?? LABEL[form]}>
      {DOTS.map(([cx, cy], i) => (
        <circle key={i} className="swarm-glyph-dot" data-lit={lit(form, i)} style={{ "--i": i } as CSSProperties} cx={cx.toFixed(2)} cy={cy.toFixed(2)} r="1.7" />
      ))}
    </svg>
  );
}
