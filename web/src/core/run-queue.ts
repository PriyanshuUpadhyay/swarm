// Run changes that Apply queued while the agent worked (design A). Each waits for its chat's turn
// to end, then starts as a normal switch. They live in memory only, so a reload drops them.

import type { SwitchCarry } from "./composer";

/** A run change that Apply queued while the agent worked. */
export type PendingSwitch = { provider: string; model: string; account: string | null; effort: string | null; carry: SwitchCarry };

export class RunQueue {
  /** Each chat's waiting change, by its session; a new object after each change. */
  state: Record<string, PendingSwitch> = {};
  private listeners = new Set<() => void>();

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  private set(state: Record<string, PendingSwitch>) {
    this.state = state;
    for (const fn of this.listeners) fn();
  }

  add(old: string, change: PendingSwitch) {
    this.set({ ...this.state, [old]: change });
  }

  cancel(old: string) {
    const { [old]: _, ...rest } = this.state;
    this.set(rest);
  }

  /** Starts each change whose agent is out of its turn. */
  run(working: (old: string) => boolean, start: (old: string, change: PendingSwitch) => void) {
    for (const [old, change] of Object.entries(this.state)) {
      if (working(old)) continue;
      // The entry goes first, so a change seen again during the switch cannot start it twice.
      this.cancel(old);
      start(old, change);
    }
  }
}
