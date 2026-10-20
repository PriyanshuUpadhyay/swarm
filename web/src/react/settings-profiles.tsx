// Profile settings and the ordered runner editor.

import { ArrowDownIcon, ArrowUpIcon, XIcon } from "lucide-react";
import { useState } from "react";
import type { App } from "../core/app";
import type { Profile, ProviderInfo, Runner } from "../core/settings";
import { Button } from "../components/ui/button";
import { FIELD, Row, useSettings } from "./settings-shared";

// ---- profiles ----

const runnerText = (r: Runner) => `${r.provider}/${r.model}/${r.effort}`;

type NameMode = { op: "new" } | { op: "rename" | "copy"; name: string };

function ProfilesPage({ app }: { app: App }) {
  const roles = useSettings(app, "roles");
  const busy = useSettings(app, "busy");
  const [naming, setNaming] = useState<NameMode | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  if (roles.state === "loading" || roles.state === "idle") return <p className="text-sm text-muted-foreground">Reading profiles…</p>;
  if (roles.state === "error") return <p className="text-sm text-failed select-text">Profiles unavailable: {roles.error}</p>;
  const data = roles.data;
  const edited = data.profiles.find((p) => p.name === editing) ?? null;
  return (
    <div className="grid min-w-0 gap-4">
      <MinUsage key={data.revision} app={app} value={data.min_usage_left_pct} />
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={busy} data-testid="profile-new" onClick={() => setNaming({ op: "new" })}>
          New profile…
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={busy}
          data-testid="profiles-reset"
          onClick={() => app.settings.ask("Reset every profile to the bundled list? Your profile changes are lost.", "Reset profiles", () => app.settings.profileOp({ op: "reset" }))}
        >
          Reset to bundled
        </Button>
      </div>
      {naming && <NameForm key={JSON.stringify(naming)} app={app} mode={naming} done={() => setNaming(null)} />}
      <ul className="grid min-w-0 gap-1" aria-label="Profiles" data-testid="profile-list">
        {data.profiles.map((p) => (
          <li key={p.name} className="grid min-w-0 gap-2 rounded-md border border-border px-3 py-2" data-testid="profile-row" data-name={p.name}>
            <div className="flex min-w-0 items-center gap-2">
              <div className="min-w-0 flex-1">
                <div className="font-mono text-sm">{p.name}</div>
                <div className="truncate text-xs text-muted-foreground">{p.runners.map(runnerText).join(" → ") || "No runners"}</div>
              </div>
              <Button size="sm" variant="ghost" disabled={busy} data-action="edit" onClick={() => setEditing(editing === p.name ? null : p.name)}>
                Edit
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} data-action="rename" onClick={() => setNaming({ op: "rename", name: p.name })}>
                Rename
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} data-action="copy" onClick={() => setNaming({ op: "copy", name: p.name })}>
                Copy
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                data-action="delete"
                onClick={() => app.settings.ask(`Delete the profile “${p.name}”?`, "Delete profile", () => app.settings.profileOp({ op: "delete", name: p.name }))}
              >
                Delete
              </Button>
            </div>
            {edited === p && <ProfileEditor key={data.revision} app={app} profile={p} done={() => setEditing(null)} />}
          </li>
        ))}
      </ul>
    </div>
  );
}

function MinUsage({ app, value }: { app: App; value: number }) {
  const busy = useSettings(app, "busy");
  const [text, setText] = useState(String(value));
  const pct = Number(text);
  const valid = text !== "" && Number.isInteger(pct) && pct >= 0 && pct <= 100;
  const save = () => valid && pct !== value && void app.settings.profileOp({ op: "min_usage", pct });
  return (
    <Row title="Usage left" detail="A runner is skipped when every account of its provider has less than this left.">
      <span className="flex items-center gap-1 text-sm">
        <input
          className={`${FIELD} w-16 text-right`}
          aria-label="Usage left minimum, percent"
          data-testid="min-usage"
          inputMode="numeric"
          value={text}
          disabled={busy}
          onChange={(e) => setText(e.target.value.replace(/\D/g, ""))}
          onBlur={save}
          onKeyDown={(e) => e.key === "Enter" && save()}
        />
        %
      </span>
    </Row>
  );
}

function NameForm({ app, mode, done }: { app: App; mode: NameMode; done: () => void }) {
  const busy = useSettings(app, "busy");
  const [name, setName] = useState(mode.op === "new" ? "" : mode.op === "copy" ? `${mode.name}.copy` : mode.name);
  const title = mode.op === "new" ? "New profile" : mode.op === "copy" ? `Copy “${mode.name}” as` : `Rename “${mode.name}” to`;
  const submit = async () => {
    const to = name.trim();
    if (!to) return;
    const ok = await app.settings.profileOp(mode.op === "new" ? { op: "new", name: to } : { op: mode.op, name: mode.name, to });
    if (ok) done();
  };
  return (
    <form
      className="flex items-end gap-2"
      data-testid="profile-name-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <label className="grid min-w-0 flex-1 gap-1 text-xs text-muted-foreground">
        {title}
        <input autoFocus className={`${FIELD} font-mono`} data-testid="profile-name" value={name} disabled={busy} onChange={(e) => setName(e.target.value)} placeholder="review.quick" />
      </label>
      <Button size="sm" type="submit" disabled={busy || !name.trim()}>
        Save
      </Button>
      <Button size="sm" type="button" variant="outline" onClick={done}>
        Cancel
      </Button>
    </form>
  );
}

/** The runners of one profile, tried in order. Sandbox, approval and permission are kept as they
 *  are unless the provider changes. */
function ProfileEditor({ app, profile, done }: { app: App; profile: Profile; done: () => void }) {
  const busy = useSettings(app, "busy");
  const providers = useSettings(app, "providers");
  const [runners, setRunners] = useState<Runner[]>(profile.runners);
  const info = (id: string): ProviderInfo | undefined => providers.find((p) => p.id === id);
  const change = (i: number, next: Runner) => setRunners(runners.map((r, j) => (j === i ? next : r)));
  const move = (i: number, by: number) => {
    const next = [...runners];
    [next[i], next[i + by]] = [next[i + by], next[i]];
    setRunners(next);
  };
  const save = async () => {
    if (await app.settings.profileOp({ op: "save", profile: { name: profile.name, runners } })) done();
  };
  return (
    <div className="grid gap-2 border-t border-border pt-2" data-testid="profile-editor">
      {runners.map((r, i) => {
        const efforts = info(r.provider)?.efforts ?? [r.effort];
        const extras = (["sandbox", "approval", "permission"] as const).filter((k) => r[k]).map((k) => `${k}=${r[k]}`);
        return (
          <div key={i} className="flex flex-wrap items-center gap-1.5" data-testid="runner">
            <span className="w-4 text-xs text-faint-foreground">{i + 1}</span>
            <select
              className={FIELD}
              aria-label={`Runner ${i + 1} provider`}
              data-testid="runner-provider"
              value={r.provider}
              onChange={(e) => {
                const p = info(e.target.value);
                change(i, { provider: e.target.value, model: "default", effort: p?.default_effort ?? "medium" });
              }}
            >
              {(providers.length ? providers : [{ id: r.provider, label: r.provider }]).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
            <input className={`${FIELD} w-40 font-mono`} aria-label={`Runner ${i + 1} model`} data-testid="runner-model" value={r.model} onChange={(e) => change(i, { ...r, model: e.target.value.trim() })} />
            <select className={FIELD} aria-label={`Runner ${i + 1} effort`} data-testid="runner-effort" value={r.effort} onChange={(e) => change(i, { ...r, effort: e.target.value })}>
              {(efforts.includes(r.effort) ? efforts : [r.effort, ...efforts]).map((e) => (
                <option key={e}>{e}</option>
              ))}
            </select>
            <Button size="icon-sm" variant="ghost" aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>
              <ArrowUpIcon />
            </Button>
            <Button size="icon-sm" variant="ghost" aria-label="Move down" disabled={i === runners.length - 1} onClick={() => move(i, 1)}>
              <ArrowDownIcon />
            </Button>
            <Button size="icon-sm" variant="ghost" aria-label="Remove runner" onClick={() => setRunners(runners.filter((_, j) => j !== i))}>
              <XIcon />
            </Button>
            {extras.length > 0 && <span className="font-mono text-xs text-faint-foreground">{extras.join(" ")}</span>}
          </div>
        );
      })}
      <div className="flex gap-2">
        <Button size="sm" variant="outline" data-testid="runner-add" onClick={() => setRunners([...runners, { provider: "claude", model: "default", effort: info("claude")?.default_effort ?? "medium" }])}>
          Add runner
        </Button>
        <span className="flex-1" />
        <Button size="sm" variant="outline" onClick={done}>
          Cancel
        </Button>
        <Button size="sm" data-testid="profile-save" disabled={busy || runners.some((r) => !r.model)} onClick={() => void save()}>
          Save
        </Button>
      </div>
    </div>
  );
}

export { ProfilesPage };
