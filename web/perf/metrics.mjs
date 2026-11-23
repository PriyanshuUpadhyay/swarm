export const percentile = (xs, q) => {
  if (!xs.length) return NaN;
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(q * sorted.length) - 1)];
};

export function giantOpenP95(records, target, count) {
  const opens = records.filter((r) => r.kind === "open" && r.target === target);
  if (opens.length !== count) throw new Error(`Expected ${count} cold giant opens; measured ${opens.length}.`);
  return percentile(opens.map((r) => r.painted - r.start), 0.95);
}
