// A .node.test.ts runs the kit script, so tsconfig.node.json checks it with Node's types.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { crossings } from "./skill-crossings";
import { layoutSkill } from "./skill-layout";
import type { Kit } from "./skills";

// The real kit, from the script the Skills page reads, so a new preset is tested on its own.
const script = fileURLToPath(new URL("../../../skills/kit/skills/swarm/scripts/preset.py", import.meta.url));
const kit: Kit = JSON.parse(execFileSync("python3", ["-I", "-B", script, "--json"], { encoding: "utf8" }));

describe("layoutSkill", () => {
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

  it("separates stages and routes edges around boxes in a branching preset", () => {
    const preset = kit.presets.find((p) => p.preset === "implement")!;
    const laid = layoutSkill(preset.steps, preset.steps.map(() => ({ w: 236, h: 96 })));
    const centers = laid.boxes.map((box) => box.y + box.h / 2);
    const stages = [...new Set(preset.steps.map((step) => step.stage))].sort((a, b) => a - b);
    const rows = stages.map((stage) => [...new Set(centers.filter((_, i) => preset.steps[i].stage === stage))]);
    expect(rows.every((row) => row.length === 1)).toBe(true);
    const ordered = rows.map(([center]) => center);
    expect(ordered).toEqual([...new Set(ordered)].sort((a, b) => a - b));
    expect(crossings(laid)).toEqual([]);
  });
});
