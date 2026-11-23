import type { PerfRecord } from "../src/core/perf";

export function percentile(xs: number[], q: number): number;
export function giantOpenP95(records: PerfRecord[], target: string, count: number): number;
