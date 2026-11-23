import { expect, it } from "vitest";
import { giantOpenP95 } from "../../perf/metrics.mjs";
import type { PerfRecord } from "./perf";

it("counts the slow cold giant open, not warm switches or other chats", () => {
  const giant = "giant";
  const records: PerfRecord[] = [
    { kind: "open", target: giant, start: 0, painted: 250 },
    ...Array.from({ length: 19 }, (_, i): PerfRecord => ({
      kind: i % 2 ? "open" : "switch",
      target: i % 2 ? "control" : giant,
      start: 300 + i * 50,
      painted: 320 + i * 50,
    })),
  ];
  expect(giantOpenP95(records, giant, 1)).toBe(250);
});
