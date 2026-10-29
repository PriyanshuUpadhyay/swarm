import { describe, expect, it } from "vitest";
import type { App } from "./app";
import { Notices, noticesCard, type NoticeHealth, type NoticesState } from "./notices";

const health = (state: string, action: NoticeHealth["action"] = null): NoticeHealth => ({ state, reason: "", steps: [], action });
const ready = (h: NoticeHealth, change: Partial<NoticesState> = {}): NoticesState => ({ health: { state: "ready", data: h }, pending: null, hidden: false, fixing: false, ...change });

/** An App stand-in: `get` answers with `read` (held by default), `post` with `answer`; every call is recorded. */
function fakeApp(answer: () => Promise<unknown>, read: () => Promise<NoticeHealth> = async () => health("held", "focus")) {
  const calls: string[] = [];
  const app = {
    api: {
      get: (path: string) => (calls.push(`GET ${path}`), read()),
      post: (path: string, body: Record<string, unknown>) => (calls.push(`POST ${path} ${JSON.stringify(body)}`), answer()),
    },
    notice: (text: string) => calls.push(`notice ${text}`),
  };
  return { notices: new Notices(app as unknown as App), calls };
}

describe("notices card", () => {
  it("shows a fix for allow, blocked and held, and nothing for ok, unknown, missing or a new state", () => {
    for (const s of ["allow", "blocked", "held"]) {
      const state = ready(health(s));
      // The same object each call, or React's useSyncExternalStore renders without end.
      expect(noticesCard(state)).toBe(state.health.state === "ready" && state.health.data);
    }
    for (const s of ["ok", "unknown", "missing", "later"]) expect(noticesCard(ready(health(s)))).toBeNull();
    // After a fix, unknown keeps the card, so the owner reads why the fix gave no answer.
    expect(noticesCard(ready(health("unknown"), { fixing: true }))).toEqual(health("unknown"));
    expect(noticesCard(ready(health("held"), { hidden: true }))).toBeNull();
    expect(noticesCard({ health: { state: "error", error: "x" }, pending: null, hidden: false, fixing: false })).toBeNull();
  });

  it("reads at most once each 30 s", async () => {
    const { notices, calls } = fakeApp(async () => health("ok"));
    await notices.load();
    await notices.load();
    expect(calls).toEqual(["GET /api/notices"]);
    expect(noticesCard(notices.state)).toEqual(health("held", "focus"));
  });

  it("says notices work once a fix made them ok, and reads again on the next return", async () => {
    const { notices, calls } = fakeApp(async () => health("ok"));
    await notices.fix("focus");
    expect(noticesCard(notices.state)).toBe("fixed");
    expect(notices.state.pending).toBeNull();
    await notices.load();
    expect(calls).toEqual(['POST /api/notices {"action":"focus"}', "GET /api/notices"]);
  });

  it("a failed fix shows a notice and keeps the card", async () => {
    const { notices, calls } = fakeApp(async () => {
      throw new Error("no helper");
    });
    await notices.load();
    await notices.fix("focus");
    expect(calls).toContain("notice Notices: no helper");
    expect(notices.state.pending).toBeNull();
    expect(noticesCard(notices.state)).toEqual(health("held", "focus"));
  });

  it("a read that started before a fix cannot write over the fix's answer", async () => {
    let answerRead: (h: NoticeHealth) => void = () => {};
    const slowRead = () => new Promise<NoticeHealth>((resolve) => {
      answerRead = resolve;
    });
    const { notices } = fakeApp(async () => health("ok"), slowRead);
    const read = notices.load();
    await notices.fix("test");
    answerRead(health("held", "focus"));
    await read;
    expect(noticesCard(notices.state)).toBe("fixed");
  });
});

it("keeps dismissal after a reload and clears it when health changes", async () => {
  const memory = new Map<string, string>();
  const old = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: {
    getItem: (key: string) => memory.get(key) ?? null,
    setItem: (key: string, value: string) => memory.set(key, value),
    removeItem: (key: string) => memory.delete(key),
  } });
  try {
    const first = fakeApp(async () => health("ok")).notices;
    await first.load();
    first.hide();
    const second = fakeApp(async () => health("ok")).notices;
    await second.load();
    expect(noticesCard(second.state)).toBeNull();
    const changed = fakeApp(async () => health("ok"), async () => health("blocked")).notices;
    await changed.load();
    expect(noticesCard(changed.state)).toEqual(health("blocked"));
    expect(memory.has("swarm.notices.dismissed")).toBe(false);
  } finally {
    if (old) Object.defineProperty(globalThis, "localStorage", old);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
