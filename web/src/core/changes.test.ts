import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Api } from "./api";
import type { App } from "./app";
import { Changes, checkState, defaultBase, safeUrl, type DiffComment } from "./changes";
import hand from "../../../tests/fixtures/s6b/hand.json" with { type: "json" };

const file = { path: "src/app.ts", status: "M", layer: "unstaged" as const, added: 1, removed: 2 };
const list = { files: [file], refs: ["refs/heads/main"], comparison: [], identity: { head: "a", branch: "feature" } };
const diff = { ...file, patch: hand.patch, display: { lines: hand.lines } };
const range = { lines: "line 11", quoted: ["+new one"] };
const editor = { anchor: 4, cursor: 4, text: "Rename this." };
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => { resolve = yes; });
  return { promise, resolve };
};
const drain = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (key: string) => store.get(key) ?? null, setItem: (key: string, value: string) => store.set(key, value), removeItem: (key: string) => store.delete(key) });
  vi.stubGlobal("requestAnimationFrame", () => 0);
  vi.stubGlobal("document", { activeElement: null });
});
afterEach(() => vi.unstubAllGlobals());

type Op = { op: string; [key: string]: unknown };
async function harness(reply: (body: Op) => Promise<Response> = async () => Response.json(range)) {
  localStorage.removeItem("swarm.changes.v1");
  const posts: { method: string | undefined; path: string; body: Op }[] = [];
  const callbacks: (() => void)[] = [];
  let shown = diff;
  const api = new Api("token", (async (url, init) => {
    const path = String(url);
    if (path === "/api/changes/comment-range") {
      const body = JSON.parse(String(init?.body)) as Op;
      posts.push({ method: init?.method, path, body });
      return reply(body);
    }
    return Response.json(path.startsWith("/api/changes/diff") ? shown : list);
  }) as typeof fetch);
  const app = {
    api, view: { workspace: "A", openId: "chat" },
    subscribe: (fn: () => void) => { callbacks.push(fn); return () => {}; },
    chair: () => ({ disabled: null }), send: vi.fn(async (..._args: unknown[]) => true), notice: vi.fn(),
  };
  const changes = new Changes(app as unknown as App);
  changes.show("changes");
  await drain();
  return { changes, app, posts, reloadDiff: async () => { shown = { ...diff, patch: hand.patch + "\n+later" }; await changes.select(file, true); }, workspace: (value: string) => { app.view.workspace = value; callbacks.forEach((fn) => fn()); } };
}
const draft = (c: DiffComment) => ({ path: c.path, lines: c.lines, quoted: c.quoted, text: c.text });

// The range and message replies below are fixed OLD/HAND data, not a browser oracle.
describe("diff clients and draft state", () => {
  it("awaits the range reply and adds one trimmed draft through POST", async () => {
    const wait = deferred<Response>();
    const { changes, posts } = await harness(() => wait.promise);
    const padded = { ...editor, text: " Rename this. " };
    changes.setEditor(file.path, file.layer, padded);
    const first = changes.addComment(4, 4, padded.text);
    const duplicate = changes.addComment(4, 4, " Rename this. ");
    const before = { comments: changes.view.comments.map(draft), editor: changes.editor(file.path, file.layer) };
    wait.resolve(Response.json(range));
    const success = await first;
    await duplicate;
    expect({ before, success, posts, comments: changes.view.comments.map(draft) }).toEqual({ before: { comments: [], editor: padded }, success: true, posts: [{ method: "POST", path: "/api/changes/comment-range", body: { op: "range", patch: hand.patch, from: 4, to: 4 } }], comments: [hand.comments[0]] });
  });

  it("discards a late Add after editor, selection, diff or workspace changes", async () => {
    const got = [];
    for (const kind of ["text", "selection", "diff", "workspace"]) {
      const wait = deferred<Response>();
      const { changes, workspace, reloadDiff } = await harness(() => wait.promise);
      changes.setEditor(file.path, file.layer, editor);
      const pending = changes.addComment(4, 4, editor.text);
      if (kind === "workspace") workspace("B");
      if (kind === "diff") await reloadDiff();
      if (kind === "text") changes.setEditor(file.path, file.layer, { ...editor, text: "Newer text" });
      if (kind === "selection") changes.setEditor(file.path, file.layer, { ...editor, anchor: 1, cursor: 1 });
      wait.resolve(Response.json(range));
      const success = await pending;
      if (kind === "workspace") { workspace("A"); await drain(); }
      got.push({ kind, success, comments: changes.view.comments.map(draft), editor: changes.editor(file.path, file.layer) });
    }
    expect(got).toEqual(["text", "selection", "diff", "workspace"].map((kind) => ({ kind, success: false, comments: [], editor: kind === "text" ? { ...editor, text: "Newer text" } : kind === "selection" ? { ...editor, anchor: 1, cursor: 1 } : editor })));
  });

  it("keeps text and selection after failed Add and lets the owner retry", async () => {
    let fail = true;
    const { changes, app } = await harness(async () => fail ? Response.json({ error: "offline" }, { status: 503 }) : Response.json(range));
    changes.setEditor(file.path, file.layer, editor);
    const first = await changes.addComment(4, 4, editor.text);
    const kept = { editor: changes.editor(file.path, file.layer), comments: changes.view.comments.map(draft), noticed: app.notice.mock.calls.length > 0 };
    fail = false;
    const retry = await changes.addComment(4, 4, editor.text);
    expect({ first, kept, retry, comments: changes.view.comments.map(draft) }).toEqual({ first: false, kept: { editor, comments: [], noticed: true }, retry: true, comments: [hand.comments[0]] });
  });

  it("uses the server message and removes only the sent draft snapshot", async () => {
    const sent = deferred<boolean>();
    // The fixed two-comment reply differs from the local one-draft formatter.
    const { changes, app, posts } = await harness(async (body) => Response.json(body.op === "message" ? { message: hand.message } : range));
    await changes.addComment(4, 4, editor.text);
    app.send.mockImplementation(() => sent.promise);
    const pending = changes.sendComments();
    await drain();
    await changes.addComment(4, 4, "New draft");
    sent.resolve(true);
    await pending;
    expect({ messagePosts: posts.filter((p) => p.body.op === "message"), sends: app.send.mock.calls, comments: changes.view.comments.map(draft), sending: changes.view.sending }).toEqual({ messagePosts: [{ method: "POST", path: "/api/changes/comment-range", body: { op: "message", comments: [hand.comments[0]] } }], sends: [["chat", hand.message, hand.message, false]], comments: [{ ...hand.comments[0], text: "New draft" }], sending: false });
  });

  it("keeps drafts and clears sending after format failure, send failure or a changed destination", async () => {
    const got = [];
    for (const kind of ["format", "send-false", "send-throw", "destination"]) {
      const wait = deferred<Response>();
      const { changes, app } = await harness(async (body) => {
        if (body.op !== "message") return Response.json(range);
        if (kind === "format") return Response.json({ error: "offline" }, { status: 503 });
        return kind === "destination" ? wait.promise : Response.json({ message: hand.message });
      });
      await changes.addComment(4, 4, editor.text);
      if (kind === "send-false") app.send.mockResolvedValue(false);
      if (kind === "send-throw") app.send.mockRejectedValue(new Error("offline"));
      app.notice.mockClear();
      const pending = changes.sendComments();
      if (kind === "destination") { app.view.openId = "other-chat"; wait.resolve(Response.json({ message: hand.message })); }
      let rejected = false;
      try { await pending; } catch { rejected = true; }
      got.push({ kind, comments: changes.view.comments.map(draft), sending: changes.view.sending, rejected, sent: app.send.mock.calls.length > 0, noticed: app.notice.mock.calls.length > 0 });
    }
    expect(got).toEqual(["format", "send-false", "send-throw", "destination"].map((kind) => ({ kind, comments: [hand.comments[0]], sending: false, rejected: false, sent: kind.startsWith("send-"), noticed: kind !== "destination" })));
  });
});

describe("branch base", () => {
  it("prefers the remote default branch, then main or master", () => {
    expect(defaultBase(["refs/heads/feature", "refs/heads/main", "refs/remotes/origin/main"], "feature")).toBe("refs/remotes/origin/main");
    expect(defaultBase(["refs/heads/feature", "refs/heads/master"], "feature")).toBe("refs/heads/master");
  });

  it("never compares a branch with itself", () => {
    expect(defaultBase(["refs/heads/main"], "main")).toBeNull();
    expect(defaultBase(["refs/heads/main", "refs/heads/topic"], "main")).toBe("refs/heads/topic");
  });
});

describe("checks", () => {
  it("reads CheckRun conclusions and StatusContext states", () => {
    expect(checkState({ status: "COMPLETED", conclusion: "SUCCESS" })).toBe("passed");
    expect(checkState({ status: "COMPLETED", conclusion: "FAILURE" })).toBe("failed");
    expect(checkState({ status: "IN_PROGRESS", conclusion: "" })).toBe("pending");
    expect(checkState({ state: "ERROR" })).toBe("failed");
    expect(checkState({ status: "COMPLETED", conclusion: "SKIPPED" })).toBe("skipped");
  });

  it("opens only http(s) links", () => {
    expect(safeUrl("https://github.com/o/r/actions/runs/1")).toBe("https://github.com/o/r/actions/runs/1");
    expect(safeUrl("javascript:alert(1)")).toBeNull();
    expect(safeUrl(undefined)).toBeNull();
  });
});
