// Setup settings and doctor results.

import { RefreshCwIcon } from "lucide-react";
import type { App } from "../core/app";
import { setUpAll } from "../core/settings";
import { Button } from "../components/ui/button";
import { Row, useSettings } from "./settings-shared";

// ---- setup & doctor ----

const STATE_TEXT = { ok: "OK", setup: "Needs setup", broken: "Needs you" } as const;

/** `swarm doctor`'s rows, each with its own Set up, and what Set up all runs. A group with a
 *  conflict needs the owner's own edit, so neither button runs it. The settings methods keep their
 *  errors in state, which the page shows, so a click does not wait for them. */
function SetupPage({ app }: { app: App }) {
  const doctor = useSettings(app, "doctor");
  const plan = useSettings(app, "plan");
  const busy = useSettings(app, "busy");
  const pending = useSettings(app, "pending");
  if (doctor.state === "error")
    return (
      <div className="grid gap-2">
        <p role="alert" className="text-sm break-words text-failed">
          Could not run swarm doctor: {doctor.error}
        </p>
        <div>
          <Button size="sm" variant="outline" data-testid="doctor-retry" onClick={() => void app.settings.loadSetup()}>
            Try again
          </Button>
        </div>
      </div>
    );
  if (doctor.state !== "ready")
    return (
      <p role="status" className="text-sm text-muted-foreground">
        Checking…
      </p>
    );
  const p = plan.state === "ready" ? plan.data : null;
  const all = p ? setUpAll(doctor.data, p) : null;
  const conflicted = new Set(p?.conflicts.map((c) => c.group));
  return (
    <div className="grid gap-4" data-testid="setup-page">
      <div className="flex items-center gap-2">
        <p role="status" className="flex-1 text-sm text-muted-foreground">
          {busy ? "Working…" : "What swarm sets up on this Mac."}
        </p>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void app.settings.loadSetup()}>
          <RefreshCwIcon />
          Check again
        </Button>
        <Button size="sm" data-testid="setup-all" disabled={busy || !all || all.length === 0} onClick={() => all && void app.settings.setUp(all)}>
          Set up all
        </Button>
      </div>
      {pending && (
        <section className="grid gap-2 rounded-md border p-3" data-testid="setup-preview">
          <h3 className="text-sm font-semibold">Set up {pending.items.join(", ")}</h3>
          {pending.files.length > 0 && (
            <ul className="grid gap-1 font-mono text-xs break-all select-text" data-testid="setup-files">
              {pending.files.map((f, i) => (
                <li key={i}>{f.path}</li>
              ))}
            </ul>
          )}
          {pending.commands.length > 0 && (
            <ul className="grid gap-1 font-mono text-xs break-all text-muted-foreground select-text" data-testid="setup-commands">
              {pending.commands.map((c, i) => (
                <li key={i}>{c.line}</li>
              ))}
            </ul>
          )}
          <div className="flex gap-2">
            <Button size="sm" data-testid="setup-apply" disabled={busy} autoFocus onClick={() => void app.settings.applySetUp()}>
              Apply
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => app.settings.cancelSetUp()}>
              Cancel
            </Button>
          </div>
        </section>
      )}
      {doctor.data.items.map((item) => (
        <div key={item.id} data-testid="doctor-item" data-id={item.id} data-state={item.state}>
          <Row title={item.title} detail={item.reason} live>
            <span className={`text-xs ${item.state === "ok" ? "text-muted-foreground" : item.state === "broken" ? "text-failed" : ""}`}>{STATE_TEXT[item.state]}</span>
            {item.state === "setup" && item.group && (
              <Button size="sm" variant="outline" data-testid="setup-item" disabled={busy || conflicted.has(item.group)} title={conflicted.has(item.group) ? "A conflict below needs your edit first" : undefined} onClick={() => void app.settings.setUpItem(item)}>
                Set up
              </Button>
            )}
          </Row>
        </div>
      ))}
      {plan.state === "error" && (
        <p role="alert" className="text-sm break-words text-failed">
          Could not plan setup: {plan.error}
        </p>
      )}
      {p && p.conflicts.length > 0 && (
        <section className="grid gap-2" data-testid="setup-conflicts">
          <h3 className="text-sm font-semibold">Conflicts</h3>
          {p.conflicts.map((c, i) => (
            <div key={i} className="text-sm">
              <div className="font-mono text-xs break-all">{c.file}</div>
              <div className="text-xs break-words text-muted-foreground">
                {c.entry}: {c.fix}
              </div>
            </div>
          ))}
        </section>
      )}
      {p && p.skipped.length > 0 && (
        <details data-testid="setup-skipped">
          <summary className="cursor-pointer text-sm">Skipped · {p.skipped.length}</summary>
          <ul className="mt-2 grid gap-1 text-xs break-words text-muted-foreground select-text">
            {p.skipped.map((s, i) => (
              <li key={i}>
                {s.group}: {s.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
      <ConductorImport app={app} />
    </div>
  );
}

/** ADR 0106: Check reads what `swarm import conductor` would add, and Import applies that plan. */
function ConductorImport({ app }: { app: App }) {
  const plan = useSettings(app, "conductor");
  const busy = useSettings(app, "busy");
  const p = plan.state === "ready" ? plan.data : null;
  const open = p?.chats.filter((c) => c.open).length ?? 0;
  return (
    <section className="grid gap-2 border-t pt-4" data-testid="conductor-import">
      <div className="flex items-center gap-2">
        <div className="flex-1">
          <h3 className="text-sm font-semibold">Import from Conductor</h3>
          <p role="status" className="text-xs text-muted-foreground">
            {plan.state === "loading"
              ? "Reading Conductor…"
              : p
                ? p.chats.length === 0
                  ? "Nothing new to import."
                  : `${p.chats.length} chats: ${open} open tabs, ${p.chats.length - open} to Recently closed. ${p.skipped.length} skipped.`
                : "Copies Conductor's chats and tabs into swarm. Conductor is not changed."}
          </p>
        </div>
        <Button size="sm" variant="outline" data-testid="conductor-check" disabled={busy || plan.state === "loading"} onClick={() => void app.settings.loadConductor()}>
          Check
        </Button>
        <Button size="sm" data-testid="conductor-apply" disabled={busy || !p || p.chats.length === 0} onClick={() => void app.settings.importConductor()}>
          Import
        </Button>
      </div>
      {plan.state === "error" && (
        <p role="alert" className="text-sm break-words text-failed">
          Could not read Conductor: {plan.error}
        </p>
      )}
      {p && p.chats.length > 0 && (
        <ul className="grid gap-1 text-xs break-words select-text" data-testid="conductor-chats">
          {p.chats.map((c) => (
            <li key={c.conductorId}>
              {c.title || c.conductorId} · {c.provider} · {c.open ? "open" : "closed"} · {c.messages} messages
              {!c.nativeLog && <span className="text-muted-foreground"> · session rebuilt from Conductor</span>}
            </li>
          ))}
        </ul>
      )}
      {p && p.skipped.length > 0 && (
        <details data-testid="conductor-skipped">
          <summary className="cursor-pointer text-xs">Skipped · {p.skipped.length}</summary>
          <ul className="mt-2 grid gap-1 text-xs break-words text-muted-foreground select-text">
            {p.skipped.map((s) => (
              <li key={s.conductorId}>
                {s.title || s.conductorId}: {s.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

export { SetupPage };
