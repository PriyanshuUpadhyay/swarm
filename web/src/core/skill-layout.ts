// Lays out one skill's steps top to bottom with Pele's layered layout (ADR 0111). Python owns every
// fact: each edge comes from a step's `uses`, and each edge's minimum length is the stage gap, so
// the rank of each step is its Python stage. The layout only orders steps in a row and routes edges.

import { layered, ledge, lnode } from "../vendor/pele-layered";

export type LayoutStep = { file: string; uses: string[]; stage: number };
export type Size = { w: number; h: number };
export type Point = { x: number; y: number };
/** A box by its top-left corner. */
export type Placed = { file: string; x: number; y: number; w: number; h: number };
export type Laid = { width: number; height: number; boxes: Placed[]; edges: { from: string; to: string; points: Point[] }[] };

const GAPS = { nodeSep: 24, edgeSep: 12, rankSep: 48, portSep: 20, tracks: true };

export function layoutSkill(steps: LayoutStep[], sizes: Size[]): Laid {
  const index = new Map(steps.map((s, i) => [s.file, i]));
  const nodes = sizes.map((s) => lnode(s.w, s.h));
  const pairs = steps.flatMap((s, to) => s.uses.map((u) => [index.get(u)!, to] as const));
  const edges = pairs.map(([from, to]) => ledge(from, to, steps[to].stage - steps[from].stage));
  const { width, height } = layered(nodes, edges, GAPS);
  return {
    width,
    height,
    boxes: nodes.map((n, i) => ({ file: steps[i].file, x: n.x - n.w / 2, y: n.y - n.h / 2, w: n.w, h: n.h })),
    edges: edges.map((e, i) => ({
      from: steps[pairs[i][0]].file,
      to: steps[pairs[i][1]].file,
      points: Array.from({ length: e.points.length / 2 }, (_, k) => ({ x: e.points[2 * k], y: e.points[2 * k + 1] })),
    })),
  };
}
