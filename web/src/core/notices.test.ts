import { describe, expect, it } from "vitest";
import type { App } from "./app";
import { Notices, noticesCard, type NoticeHealth, type NoticesState } from "./notices";

const health = (state: string, action: NoticeHealth["action"] = null): NoticeHealth => ({ state, reason: "", steps: [], action });
const ready = (h: NoticeHealth, change: Partial<NoticesState> = {}): NoticesState => ({ health: { state: "ready", data: h }, pending: null, hidden: false, fixing: false, ...change });

/** An App stand-in whose `post` answers with `answer`; every call is recorded. */
function fakeApp(answer: () => Promise<unknown>) {
  const calls: string[] = [];
  const app = {
    api: {
      get: async (path: string) => (calls.push(`GET ${path}`), health("held", "focus")),
      post: (path: string, body: Record<string, unknown>) => (calls.push(`POST ${path} ${JSON.stringify(body)}`), answer()),
    },
    notice: (text: string) => calls.push(`notice ${text}`),
  };
  return { notices: new Notices(app as unknown as App), calls };
}

describe("notices card", () => {
  it("shows a fix for allow, blocked and held, and nothing for ok, unknown, missing or a new state", () => {
    for (const s of ["allow", "blocked", "held"]) expect(noticesCard(ready(health(s)))).toEqual(health(s));
    for (const s of ["ok", "unknown", "missing", "later"]) expect(noticesCard(ready(health(s)))).toBeNull();
    expect(noticesCard(ready(health("held"), { hidden: true }))).toBeNull();
    expect(noticesCard({ health: { state: "error", error: "x" }, pending: null, hidden: false, fixing: false })).toBeNull();
  });

  it("reads at most once each 30 s unless forced", async () => {
    const { notices, calls } = fakeApp(async () => health("ok"));
    await notices.load(true);
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
    await notices.fix("allow");
    expect(calls).toContain("notice Notices: no helper");
    expect(notices.state.pending).toBeNull();
  });
});
