// S10e-1 test support: a real Api over a stubbed fetch. The rows page and the expected matches come
// from the ORIGINAL capture (tests/serve/find_original); POST /api/rows waits until a test answers it.
import { vi } from "vitest";
import inputs from "../../../tests/serve/find_original/inputs-lines.json";
import original from "../../../tests/serve/find_original/matches-07.json";
import { Api, type Row, type RowPage } from "./api";
import { App } from "./app";

const reveal = inputs.find((c) => c.id === "lines-app-reveal")!;
/** The ORIGINAL answer for query "parser": the step `parser-check` inside the closed fold `run`. */
export const ORIGINAL_PARSER = original.find((c) => c.id === "lines-app-reveal")!.matches;
/** HAND answer for query "read" on the same rows: `read-config` has "read" at offset 0 of its header. */
export const HAND_READ = [{ id: "read-config", fold: "run", offset: 0, occurrence: 0 }];

export type Posted = { body: { version: number; query: string; rows: Row[]; find: unknown; full: unknown }; signal?: AbortSignal | null; answer: (res: Response) => void };

export const matchesReply = (matches: unknown[]) => Response.json({ version: 1, matches });
export const errorReply = (status: number, error: string) => Response.json({ error }, { status });

export async function waitFor(done: () => boolean, ms = 600): Promise<boolean> {
  for (let waited = 0; waited < ms; waited += 5) {
    if (done()) return true;
    await new Promise((r) => setTimeout(r, 5));
  }
  return done();
}

export function stubBrowser() {
  vi.stubGlobal("requestAnimationFrame", () => 0);
  vi.stubGlobal("document", { activeElement: null, querySelector: () => null });
}

export function findHarness() {
  const posts: Posted[] = [];
  const page = (): RowPage => ({ page: { start_offset: 0, end_offset: 20 }, rows: structuredClone(reveal.rows) as unknown as Row[], usage: null, find: reveal.find as RowPage["find"] });
  const api = new Api("token", (async (url, init) => {
    const path = String(url);
    if (path.startsWith("/api/rows") && init?.method === "POST") {
      return new Promise<Response>((answer) => posts.push({ body: JSON.parse(String(init.body)), signal: init.signal, answer }));
    }
    if (path.startsWith("/api/rows")) return Response.json(page());
    return new Response("{}", { status: 404 });
  }) as typeof fetch);
  const app = new App(api);
  return {
    app,
    posts,
    async open() {
      app.openChat("chat");
      await waitFor(() => app.view.lines.length > 0);
    },
    /** The request for this query, once the browser has sent it. Reading the view stands in for a React render. */
    sent: (query: string) => {
      void app.view;
      return posts.find((p) => p.body.query.trim().toLowerCase() === query);
    },
  };
}
