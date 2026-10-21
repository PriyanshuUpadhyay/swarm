// A .node.test.ts runs the kit script, so tsconfig.node.json checks it with Node's types.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { layoutSkill, type Laid, type Placed, type Size } from "./skill-layout";
import type { Kit } from "./skills";

// The real kit, from the script the Skills page reads, so a new preset is tested on its own.
const script = fileURLToPath(new URL("../../../skills/kit/skills/plan/scripts/preset.py", import.meta.url));
const kit: Kit = JSON.parse(execFileSync("python3", ["-I", "-B", script, "--json"], { encoding: "utf8" }));

/** Edges that pass through a box that is not one of their two ends. */
function crossings(laid: Laid): string[] {
  const inside = (b: Placed, x: number, y: number) => x > b.x + 1 && x < b.x + b.w - 1 && y > b.y + 1 && y < b.y + b.h - 1;
  const hits: string[] = [];
  for (const e of laid.edges)
    for (const b of laid.boxes) {
      if (b.file === e.from || b.file === e.to) continue;
      for (let i = 1; i < e.points.length; i++) {
        const [p, q] = [e.points[i - 1], e.points[i]];
        for (let t = 0; t <= 1; t += 1 / 60) if (inside(b, p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)) hits.push(`${e.from}->${e.to} via ${b.file}`);
      }
    }
  return [...new Set(hits)];
}

// Card sizes the page can draw: the trial's box, a tall card, and a narrow pane's card.
const SIZES: [string, (i: number) => Size][] = [
  ["236x96", () => ({ w: 236, h: 96 })],
  ["mixed heights", (i) => ({ w: 236, h: 72 + (i % 3) * 40 })],
  ["narrow", () => ({ w: 160, h: 120 })],
];

describe("layoutSkill", () => {
  it("reads all the kit presets", () => expect(kit.presets.length).toBeGreaterThanOrEqual(8));

  it("keeps a source that feeds only the last step in stage 0, as Python's stage puts it", () => {
    const steps = [
      { file: "01-a", uses: [], stage: 0 },
      { file: "02-b", uses: ["01-a"], stage: 1 },
      { file: "03-c", uses: ["02-b"], stage: 2 },
      { file: "04-x", uses: [], stage: 0 },
      { file: "05-end", uses: ["03-c", "04-x"], stage: 3 },
    ];
    const laid = layoutSkill(steps, steps.map(() => ({ w: 200, h: 80 })));
    expect(laid.boxes[3].y).toBe(laid.boxes[0].y);
    expect(crossings(laid)).toEqual([]);
  });

  it("counts an edge that runs through another box", () => {
    const box = (file: string, y: number) => ({ file, x: 0, y, w: 100, h: 50 });
    const laid = { width: 100, height: 300, boxes: [box("a", 0), box("b", 100), box("c", 200)], edges: [{ from: "a", to: "c", points: [{ x: 50, y: 50 }, { x: 50, y: 200 }] }] };
    expect(crossings(laid)).toEqual(["a->c via b"]);
  });
  for (const preset of kit.presets)
    for (const [name, size] of SIZES)
      it(`${preset.preset} at ${name}: each stage is one row and no edge runs through a box`, () => {
        const laid = layoutSkill(preset.steps, preset.steps.map((_, i) => size(i)));
        const top = new Map<number, number>();
        for (const [i, b] of laid.boxes.entries()) {
          const stage = preset.steps[i].stage;
          const row = b.y + b.h / 2;
          if (!top.has(stage)) top.set(stage, row);
          expect(row, `${b.file} is in the row of stage ${stage}`).toBe(top.get(stage));
        }
        const rows = [...top.entries()].sort((a, b) => a[0] - b[0]).map(([, y]) => y);
        expect(rows).toEqual([...new Set(rows)].sort((a, b) => a - b));
        expect(crossings(laid)).toEqual([]);
      });
});
