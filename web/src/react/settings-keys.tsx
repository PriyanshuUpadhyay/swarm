// API keys that the kit's tools read from the Keychain. The page shows only whether each key is
// saved; a typed value lives in this row until Save sends it, then the field is cleared.

import { useState } from "react";
import type { App } from "../core/app";
import type { ApiKey } from "../core/settings";
import { Button } from "../components/ui/button";
import { FIELD, useSettings } from "./settings-shared";

function KeysPage({ app }: { app: App }) {
  const keys = useSettings(app, "keys");
  if (keys.state === "error")
    return (
      <div className="grid gap-2">
        <p role="alert" className="text-sm break-words text-failed">
          Could not read the keys: {keys.error}
        </p>
        <div>
          <Button size="sm" variant="outline" onClick={() => void app.settings.loadKeys()}>
            Try again
          </Button>
        </div>
      </div>
    );
  if (keys.state !== "ready")
    return (
      <p role="status" className="text-sm text-muted-foreground">
        Reading the Keychain…
      </p>
    );
  return (
    <div className="grid gap-5" data-testid="keys-page">
      <p className="text-sm text-muted-foreground">
        Each key is kept in your login Keychain as <code className="font-mono">swarm-NAME</code>. Every pane reads it from there, and an environment variable with the same name wins.
      </p>
      {keys.data.keys.map((key) => (
        <KeyRow key={key.name} app={app} apiKey={key} />
      ))}
    </div>
  );
}

function KeyRow({ app, apiKey: key }: { app: App; apiKey: ApiKey }) {
  const busy = useSettings(app, "busy");
  const [value, setValue] = useState("");
  const id = `key-${key.name}`;
  const save = async () => {
    if (await app.settings.saveKey(key.name, value.trim())) setValue("");
  };
  return (
    <section className="grid gap-2" data-testid="key-row" data-name={key.name} data-saved={key.saved}>
      <div className="flex items-baseline gap-2">
        <label htmlFor={id} className="text-sm font-semibold">
          {key.label}
        </label>
        <code className="font-mono text-xs text-muted-foreground">{key.name}</code>
        <span className={`ml-auto text-xs ${key.saved ? "text-muted-foreground" : ""}`} role="status">
          {key.saved ? "Saved" : "Not saved"}
        </span>
      </div>
      <p className="text-xs break-words text-muted-foreground">
        {key.used_for}{" "}
        <a className="underline hover:text-foreground" href={key.get_url} target="_blank" rel="noopener noreferrer">
          Get a key
        </a>
      </p>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) void save();
        }}
      >
        <input
          id={id}
          type="password"
          autoComplete="off"
          spellCheck={false}
          className={`${FIELD} flex-1 font-mono text-xs`}
          placeholder={key.saved ? "Paste a new key to replace it" : "Paste the key"}
          value={value}
          disabled={busy}
          onChange={(e) => setValue(e.target.value)}
          data-testid="key-input"
        />
        <Button size="sm" type="submit" disabled={busy || !value.trim()} data-testid="key-save">
          {key.saved ? "Replace" : "Add"}
        </Button>
        {key.saved && (
          <Button size="sm" type="button" variant="outline" disabled={busy} data-testid="key-remove" onClick={() => app.settings.removeKey(key.name, key.label)}>
            Remove
          </Button>
        )}
      </form>
    </section>
  );
}

export { KeysPage };
