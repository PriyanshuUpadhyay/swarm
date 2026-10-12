// Hooks for the budget suite. Both apps push records with the same meaning.

export type PerfKind = "key" | "switch" | "open";
export type PerfRecord = { kind: PerfKind; target: string; start: number; painted: number };

declare global {
  interface Window {
    __swarmPerf: PerfRecord[];
    __swarmReady?: boolean;
  }
}

/** Runs `done` inside the first animation frame where `ready()` holds. Gives up after about 10 s. */
export function whenPainted(ready: () => boolean, done: () => void) {
  let frames = 0;
  const tick = () => {
    if (ready()) done();
    else if (++frames < 600) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

// Every key press adds a record, so an app left open must keep only the newest ones.
export const MAX_RECORDS = 1000;

/** `start` is the KeyboardEvent.timeStamp; `painted` is performance.now() in that frame. */
export function measure(kind: PerfKind, target: string, start: number, ready: () => boolean) {
  whenPainted(ready, () => {
    const records = (window.__swarmPerf ??= []);
    records.push({ kind, target, start, painted: performance.now() });
    if (records.length > MAX_RECORDS) records.splice(0, records.length - MAX_RECORDS);
  });
}
