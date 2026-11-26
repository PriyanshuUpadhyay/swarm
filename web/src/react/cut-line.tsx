export const plural = (n: number, word: string) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;

export function sourceLines(text: string): string[] {
  const lines = text.split(/\r\n|[\n\r]/);
  if (lines.at(-1) === "") lines.pop();
  return lines;
}

export function CutLine({ count, show, busy, error, testid }: { count: string; show: () => void; busy?: boolean; error?: string; testid?: string }) {
  return (
    <div className="cut-line">
      <span>… {count}</span>
      <button type="button" className="t-link" data-testid={testid} disabled={busy} onClick={show}>{busy ? "Loading…" : "Show all"}</button>
      {error && <span className="text-failed">{error}</span>}
    </div>
  );
}
