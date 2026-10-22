// The composer tests' app harness, shared by composer.test.ts and composer-queue.test.ts.
import { ApiError, type Agent, type Api, type Row, type RowPage, type Snapshot } from "./api";
import { App } from "./app";
import { chair, session } from "./chat-fixtures";

export const user = (id: string, text: string, offset = 10): Row => ({ id, kind: "user", text, order: [offset, 0] });

/** An App on a fake server: rows for one chat, a snapshot, and a log of the action POSTs. */
export function harness(fail?: (path: string) => string | null, reply: (path: string) => unknown = () => null) {
  const posts: { path: string; body: Record<string, unknown> }[] = [];
  const page: RowPage = { page: { start_offset: 0, end_offset: 10 }, rows: [user("hello", "hello", 1)], usage: null };
  const post = async (path: string, body: Record<string, unknown>) => {
    posts.push({ path, body });
    const err = fail?.(path);
    if (err) throw new ApiError(409, err);
    return reply(path);
  };
  const api = { rows: async () => page, action: post, post } as unknown as Api;
  const app = new App(api);
  const snapshot = (agent: Partial<Agent> = {}, children: Agent[] = []) =>
    (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({ version: 1, sessions: [session()], agents: { chat: [chair(agent), ...children] } });
  return { app, posts, snapshot };
}

export const settle = () => new Promise((r) => setTimeout(r, 0));
