// The update notice (ADR 0118): when the Homebrew tap has a newer swarm, one toast asks to upgrade,
// then asks to restart the `swarm ui` server, and the HUD when the upgrade changed it. The page
// reloads when the new server answers.

import { toast } from "sonner";
import { ApiError } from "../core/api";
import type { App } from "../core/app";
import { message } from "../core/app-types";
import { showError, shownErrors } from "./errors";

type Update = { current: string; latest: string | null; available: boolean };

const ID = "swarm-update";
const CHECK_MS = 6 * 3600_000;
const RESTART_MS = 30_000;
// Sonner closes a toast after its action runs; the next step reuses the toast, so it stays.
// A reused toast keeps the options it is not given, so each progress step clears the buttons.
const NO_BUTTONS = { action: undefined, cancel: undefined };
const STAY = { id: ID, duration: Infinity };
const keep = (step: () => void) => (event: { preventDefault: () => void }) => {
  event.preventDefault();
  step();
};

/** Checks on load and every 6 h until a notice shows. Returns the stop function. */
export function watchUpdates(app: App): () => void {
  let shown = false;
  let reported = false;
  const check = async () => {
    if (shown) return;
    try {
      const update = await app.api.get<Update>("/api/update");
      if (!update.available || !update.latest) return;
      shown = true;
      offer(app, update.latest);
    } catch (err) {
      // Offline or the tap is not reachable: the next check tries again. A server refusal, such
      // as a tap formula that names no version, can repeat on every check, so it is recorded once.
      if (err instanceof ApiError && !reported) {
        reported = true;
        shownErrors.report(`Update check: ${message(err)}`);
      }
    }
  };
  // Fire-and-forget is safe: check catches its own errors.
  void check();
  const timer = setInterval(() => void check(), CHECK_MS);
  return () => clearInterval(timer);
}

// A failed step shows its error in this toast, beside the button that tries again: a toast of its
// own would hide this one, since sonner shows 3 and a press can leave their timers paused.
function reportFailure(text: string) {
  shownErrors.report(text);
  return text;
}

function offer(app: App, latest: string, error?: string) {
  (error ? toast.error : toast)(`Swarm ${latest} is available`, {
    ...STAY,
    description: error ?? "Upgrade installs it with Homebrew. Then you restart the server.",
    // Fire-and-forget is safe: upgrade catches its own error and shows it in this toast.
    action: { label: "Upgrade", onClick: keep(() => void upgrade(app, latest)) },
    cancel: { label: "Not now", onClick: () => {} },
  });
}

async function upgrade(app: App, latest: string) {
  toast.loading(`Upgrading to swarm ${latest}…`, { ...STAY, description: "brew update and brew upgrade swarm can take a few minutes.", ...NO_BUTTONS });
  try {
    const { version, hud } = await app.api.post<{ version: string; hud: boolean }>("/api/update", {});
    installed(app, version, hud);
  } catch (err) {
    offer(app, latest, reportFailure(`Upgrade: ${message(err)}`));
  }
}

// `hudChanged`: the upgrade changed the HUD, and it still needs its restart.
function installed(app: App, version: string, hudChanged: boolean, error?: string) {
  const stale = hudChanged ? "The swarm ui server and the HUD still run" : "The swarm ui server still runs";
  (error ? toast.error : toast.success)(`Swarm ${version} is installed`, {
    ...STAY,
    description: `${error ? `${error}. ` : ""}${stale} the old version. Restart to use the new one.`,
    // Fire-and-forget is safe: restart shows a refusal in this toast, and its timer covers the rest.
    action: { label: "Restart server", onClick: keep(() => void restart(app, version, hudChanged)) },
    cancel: { label: "Later", onClick: () => {} },
  });
}

async function restart(app: App, version: string, hudChanged: boolean) {
  toast.loading(hudChanged ? "Restarting the HUD and the swarm ui server…" : "Restarting the swarm ui server…", { ...STAY, description: "The page reloads when the new server answers.", ...NO_BUTTONS });
  try {
    await app.api.post("/api/update/restart", {});
  } catch (err) {
    // The old server stops during this call, so only a refusal it sent is a failure. It refuses
    // before it stops, so the owner can press Restart server again. Only a 502 (the HUD's one try)
    // and a 500 (the start of the new server) come after the HUD try; then the next press restarts
    // only the server. The 409s and the guard's refusals come before it.
    if (err instanceof ApiError) {
      const tried = err.status === 500 || err.status === 502;
      return installed(app, version, hudChanged && !tried, reportFailure(`Restart: ${message(err)}`));
    }
  }
  // The stream's hello frame reloads the page on the new build, which ends this timer.
  setTimeout(() => {
    toast.dismiss(ID);
    showError(`The new swarm ui server did not answer within ${RESTART_MS / 1000} s. Run \`swarm ui\` in a terminal; its log is ui.log in the swarm folder.`);
  }, RESTART_MS);
}
