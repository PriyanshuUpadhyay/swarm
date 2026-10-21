import type { Laid, Placed } from "./skill-layout";

/** Edges that pass through a box that is not one of their ends. The layout test and the e2e both
 *  check the drawn graph with it, so they cannot disagree on what a crossing is. */
export function crossings({ boxes, edges }: Pick<Laid, "boxes" | "edges">): string[] {
  const inside = (b: Placed, x: number, y: number) => x > b.x + 1 && x < b.x + b.w - 1 && y > b.y + 1 && y < b.y + b.h - 1;
  const hits: string[] = [];
  for (const e of edges)
    for (const b of boxes) {
      if (b.file === e.from || b.file === e.to) continue;
      for (let i = 1; i < e.points.length; i++) {
        const [p, q] = [e.points[i - 1], e.points[i]];
        for (let t = 0; t <= 1; t += 1 / 60) if (inside(b, p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)) hits.push(`${e.from}->${e.to} via ${b.file}`);
      }
    }
  return [...new Set(hits)];
}
