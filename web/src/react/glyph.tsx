import type { CSSProperties } from "react";
// The glyph: a hexagon of seven dots (DESIGN.md). The rest and still working forms light the
// same three ring dots; only rest has a foreground centre. When the state is unknown, all are unlit.

export type GlyphForm = "rest" | "working" | "unlit";

const LABEL: Record<GlyphForm, string> = { rest: "Swarm", working: "Working", unlit: "State unknown" };

// Centre first, then six ring dots clockwise from the top, as on the values board.
const DOTS = [[256, 256], [256, 120], [374, 188], [374, 324], [256, 392], [138, 324], [138, 188]] as const;

export function Glyph({ form = "rest", size = 16, label, className, "aria-hidden": hidden, "data-tip": tip }: {
  form?: GlyphForm;
  size?: 16 | 12;
  label?: string;
  className?: string;
  "aria-hidden"?: boolean;
  "data-tip"?: string;
}) {
  return (
    <svg className={className ? `swarm-glyph ${className}` : "swarm-glyph"} data-form={form} width={size} height={size} viewBox="0 0 512 512" role="img" aria-label={hidden ? undefined : (label ?? LABEL[form])} aria-hidden={hidden} data-tip={tip}>
      {DOTS.map(([cx, cy], i) => (
        <circle key={i} className="swarm-glyph-dot" data-lit={form !== "unlit" && i > 0 && i <= 3} style={{ "--phase": (7 - i) % 6 } as CSSProperties} cx={cx} cy={cy} r={i === 0 ? 60 : 56} />
      ))}
    </svg>
  );
}
