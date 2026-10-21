// The Composer (ADR 0111). The form builds the text; Python checks that exact text; Copy is on only
// for text that passed. Nothing is written to disk: the owner pastes the text where it belongs.

import type { App } from "../core/app";
import { blankDraft, composable, copyable, draftText, presetDraft, type Draft, type Kit } from "../core/skills";
import { Button } from "../components/ui/button";
import { copyText } from "./copy";
import { FIELD, Segmented } from "./settings-shared";
import { SkillGraph } from "./skill-graph";
import { useSkills } from "./skills";

const COLUMNS = ["File", "Needs", "Holds", "Block", "Why"];
const NEW_ROW = ["", "", "", "", ""];

export function Composer({ app, kit }: { app: App; kit: Kit }) {
  const draft = useSkills(app, "draft");
  const check = useSkills(app, "check");
  const undo = useSkills(app, "undo");
  const text = draftText(draft);
  const fresh = check?.text === text ? check : null;
  const set = (change: Partial<Draft>) => app.skills.setDraft({ ...draft, ...change });
  const setCell = (row: number, col: number, value: string) => set({ rows: draft.rows.map((r, i) => (i === row ? r.map((c, j) => (j === col ? value : c)) : r)) });
  const presets = composable(kit);
  const origin = presets.find((p) => p.preset === draft.from);
  // A draft that differs from where it started holds the owner's edits, which a new start would drop.
  const edited = text !== draftText(origin ? presetDraft(origin) : blankDraft());
  return (
    <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
      <form className="grid min-h-0 content-start gap-4 overflow-y-auto border-r border-border p-5 text-sm" onSubmit={(e) => e.preventDefault()} data-testid="draft-form">
        <div className="flex flex-wrap items-end gap-4">
          <label className="grid gap-1">
            <span className="text-xs text-muted-foreground">Start from</span>
            <select
              className={`${FIELD} base-select`}
              data-testid="draft-from"
              value={draft.from ?? ""}
              onChange={(e) => {
                const preset = presets.find((p) => p.preset === e.target.value);
                // A confirm does not show in the app's web view, so a new start keeps the edits for Undo.
                app.skills.setDraft(preset ? presetDraft(preset) : blankDraft(), edited ? draft : null);
              }}
            >
              <option value="">A new skill</option>
              {presets.map((p) => (
                <option key={p.preset} value={p.preset}>
                  {p.preset}
                </option>
              ))}
            </select>
          </label>
          {undo && (
            <Button type="button" variant="outline" size="sm" data-testid="draft-undo" onClick={() => app.skills.setDraft(undo)}>
              Undo new start
            </Button>
          )}
          <div className="grid gap-1">
            <span className="text-xs text-muted-foreground">Stops for</span>
            <Segmented
              label="Stops for"
              testid="draft-stop"
              value={draft.stop}
              options={[
                ["owner", "The owner"],
                ["chat", "The chat"],
              ]}
              onChange={(stop) => set({ stop })}
            />
          </div>
        </div>
        {draft.from === null && (
          <div className="grid grid-cols-[12rem_minmax(0,1fr)] gap-3">
            <label className="grid gap-1">
              <span className="text-xs text-muted-foreground">Name</span>
              <input className={`${FIELD} font-mono`} name="skill-name" autoComplete="off" spellCheck={false} data-testid="draft-name" value={draft.name} onChange={(e) => set({ name: e.target.value })} />
            </label>
            <label className="grid gap-1">
              <span className="text-xs text-muted-foreground">Use when</span>
              <input className={FIELD} name="skill-description" autoComplete="off" data-testid="draft-description" value={draft.description} onChange={(e) => set({ description: e.target.value })} />
            </label>
          </div>
        )}
        <div className="grid gap-2">
          {draft.rows.map((row, i) => (
            <fieldset key={i} className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-2 rounded-lg border border-border p-2.5" data-testid="draft-row">
              <legend className="sr-only">Step {i + 1}</legend>
              {row.map((cell, j) => (
                <label key={j} className={`grid gap-1 ${j >= 2 ? "col-span-2" : ""}`}>
                  <span className="text-xs text-muted-foreground">{COLUMNS[j]}</span>
                  <input
                    className={`${FIELD} ${j === 0 || j === 3 ? "font-mono" : ""}`}
                    autoComplete="off"
                    spellCheck={j === 2 || j === 4}
                    data-col={COLUMNS[j]}
                    value={cell}
                    onChange={(e) => setCell(i, j, e.target.value)}
                  />
                </label>
              ))}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="col-start-3 row-start-1 self-end"
                aria-label={`Remove step ${i + 1}`}
                disabled={draft.rows.length === 1}
                onClick={() => set({ rows: draft.rows.filter((_, k) => k !== i) })}
              >
                Remove
              </Button>
            </fieldset>
          ))}
          <Button type="button" variant="outline" size="sm" className="w-fit" data-testid="draft-add" onClick={() => set({ rows: [...draft.rows, [...NEW_ROW]] })}>
            Add step
          </Button>
        </div>
      </form>
      <div className="grid min-h-0 grid-rows-[minmax(0,1fr)_auto_minmax(0,1fr)]">
        <div className="min-h-0 overflow-auto p-4" data-testid="draft-graph">
          {fresh && "result" in fresh && fresh.result.ok ? (
            <SkillGraph steps={fresh.result.preset.steps} />
          ) : (
            <p className="text-sm text-muted-foreground">The graph shows when the draft passes.</p>
          )}
        </div>
        <div className="flex items-center gap-3 border-y border-border px-4 py-2 text-sm">
          <p aria-live="polite" className="min-w-0 flex-1 [overflow-wrap:anywhere]" data-testid="draft-status">
            {!fresh ? (
              <span className="text-muted-foreground">Checking…</span>
            ) : "error" in fresh ? (
              <span className="text-failed">{fresh.error}</span>
            ) : fresh.result.ok ? (
              <span className="text-muted-foreground">Python passed this text.</span>
            ) : (
              <span className="text-failed">{fresh.result.defect}</span>
            )}
          </p>
          <Button
            size="sm"
            data-testid="draft-copy"
            disabled={!copyable({ draft, check })}
            onClick={() => void copyText(text, draft.from ? "Stop line and step table" : "SKILL.md")}
          >
            Copy
          </Button>
        </div>
        <pre className="min-h-0 overflow-auto p-4 font-mono text-xs whitespace-pre-wrap" data-testid="draft-text">
          {text}
        </pre>
      </div>
    </div>
  );
}
