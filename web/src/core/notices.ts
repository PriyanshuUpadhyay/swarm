import type { App } from "./app";
import type { Load } from "./changes";

/** `swarm notices health --json` (ADR 0102). More states may come; the card shows none for them. */
export type NoticeHealth = { state: string; reason: string; steps: string[]; action: NoticeAction | null };
export type NoticeAction = "allow" | "notifications" | "focus" | "test";
export type NoticesState = {
  health: Load<NoticeHealth>;
  /** The fix in flight. */
  pending: NoticeAction | null;
  /** The owner's Not now, or Done after a fix, until the page reloads. */
  hidden: boolean;
  /** A fix ran, so the card says when notices work again. */
  fixing: boolean;
};

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

/** The notice card: the health to fix, "fixed" once a fix made it ok, or null for no card. */
export function noticesCard(state: NoticesState): NoticeHealth | "fixed" | null {
  if (state.hidden || state.health.state !== "ready") return null;
  const health = state.health.data;
  if (health.state === "ok") return state.fixing ? "fixed" : null;
  return ["allow", "blocked", "held"].includes(health.state) ? health : null;
}

/** Whether macOS lets swarm's notices through, and the owner's fixes (ADR 0102). */
export class Notices {
  state: NoticesState = { health: { state: "idle" }, pending: null, hidden: false, fixing: false };
  private listeners = new Set<() => void>();
  /** When the health was last read, in ms since 1970; reads come at most once each 30 s. */
  private readAt = 0;

  constructor(private readonly app: App) {}

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private set(change: Partial<NoticesState>) {
    this.state = { ...this.state, ...change };
    for (const fn of this.listeners) fn();
  }

  /** Reads the health; on start and when the window comes back, at most once each 30 s. */
  async load(force = false) {
    if (this.state.pending || (!force && Date.now() - this.readAt < 30_000)) return;
    this.readAt = Date.now();
    try {
      this.set({ health: { state: "ready", data: await this.app.api.get<NoticeHealth>("/api/notices") } });
    } catch (err) {
      this.set({ health: { state: "error", error: message(err) } });
    }
  }

  /** Runs the card's fix: the macOS prompt, a System Settings pane, or a test notice. */
  async fix(action: NoticeAction) {
    if (this.state.pending) return;
    this.set({ pending: action, fixing: true });
    try {
      this.set({ health: { state: "ready", data: await this.app.api.post<NoticeHealth>("/api/notices", { action }) } });
    } catch (err) {
      this.app.notice(`Notices: ${message(err)}`, true);
    } finally {
      // The owner fixes a pane in System Settings, so the next return to the window reads again.
      this.readAt = 0;
      this.set({ pending: null });
    }
  }

  hide() {
    this.set({ hidden: true });
  }
}
