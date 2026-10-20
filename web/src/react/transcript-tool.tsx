// Tool cards, shell runs, and their status glyphs.
import { CheckIcon, CircleHelpIcon, CircleIcon, CircleStopIcon, XIcon } from "lucide-react";
import type { ToolState } from "../core/api";
import { duration } from "../core/transcript";
import { DiffView, OutputBox, plural } from "./transcript-output";
import type { Ctx } from "./transcript-row";

export function StatusGlyph({ state }: { state: ToolState }) {
  const label = STATE_LABEL[state];
  const cls = "glyph-slot";
  switch (state) {
    case "waiting":
      return <CircleIcon aria-label={label} className={`${cls} fill-current text-working`} />;
    case "finished":
      return <CheckIcon aria-label={label} className={`${cls} text-done`} />;
    case "failed":
      return <XIcon aria-label={label} className={`${cls} text-failed`} />;
    case "interrupted":
      return <CircleStopIcon aria-label={label} className={`${cls} text-waiting`} />;
    default:
      return <CircleHelpIcon aria-label={label} className={`${cls} text-muted-foreground`} />;
  }
}

const STATE_LABEL: Record<ToolState, string> = {
  waiting: "Waiting for result",
  finished: "Finished",
  failed: "Failed",
  interrupted: "Interrupted",
  unreported: "No result",
};

function ToolCard({ app, line, full }: Ctx) {
  const t = line.row.tool!;
  const counts = t.diffs?.length ? t.diffs.reduce((a, d) => [a[0] + d.added, a[1] + d.removed], [0, 0]) : null;
  const result = [t.exit_code !== undefined ? `exit ${t.exit_code}` : "", t.duration_seconds !== undefined ? duration(t.duration_seconds) : ""].filter(Boolean).join(" · ");
  return (
    <div className={`card${line.open ? " open" : ""}`} data-testid="tool-card" data-state={t.state}>
      <button type="button" className="card-head" aria-expanded={line.open} onClick={() => app.toggleLine(line)}>
        <StatusGlyph state={t.state} />
        <span className="shrink-0 font-semibold">{t.name}</span>
        <span className="min-w-0 flex-1 truncate text-left text-muted-foreground">{t.header_title}</span>
        {counts && (
          <span className="font-mono text-xs">
            <span className="text-done">+{counts[0]}</span> <span className="text-failed">−{counts[1]}</span>
          </span>
        )}
        {result && <span className={`num text-xs ${t.state === "failed" ? "text-failed" : "text-muted-foreground"}`}>{result}</span>}
      </button>
      {t.skill_body && (
        <details className="card-indent text-xs">
          <summary className="cursor-default text-muted-foreground">Body</summary>
          <pre className="plain">{t.skill_body.text}</pre>
        </details>
      )}
      {line.open && (
        <div className="card-body card-indent">
          {t.command && <pre className="plain">{t.command}</pre>}
          {t.path && <div className="font-mono text-xs break-all">{t.path}</div>}
          {t.output ? (
            <OutputBox app={app} preview={t.output} full={full} title="Output" />
          ) : (
            <div className="text-muted-foreground">{t.state === "waiting" ? "Waiting for tool result." : "No tool result was recorded."}</div>
          )}
          {t.diffs?.map((d) => <DiffView key={d.id} app={app} diff={d} full={full} />)}
        </div>
      )}
    </div>
  );
}

function ShellRow({ app, line, full }: Ctx) {
  const run = line.row.shell!;
  const note = run.exit_code !== undefined ? `exit ${run.exit_code}` : !run.output.text ? "no output" : `${plural(run.output.full_lines || 1, "line")}`;
  return (
    <div className="grid gap-1">
      {run.command && (
        <div className="flex items-center gap-2">
          {run.exit_code !== undefined ? <StatusGlyph state={run.exit_code === 0 ? "finished" : "failed"} /> : <span className="glyph-slot font-mono text-muted-foreground">$</span>}
          <span className="min-w-0 flex-1 font-mono break-all">{run.command}</span>
          <span className={`num text-xs ${run.exit_code === undefined ? "text-muted-foreground" : run.exit_code === 0 ? "text-done" : "text-failed"}`}>{note}</span>
        </div>
      )}
      {run.output.text && (
        <div className="card-indent">
          <OutputBox app={app} preview={run.output} full={full} title="Output" />
        </div>
      )}
    </div>
  );
}

export { ShellRow, ToolCard };
