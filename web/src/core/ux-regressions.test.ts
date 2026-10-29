import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";
import type { Api, Row, RowPage } from "./api";
import { Changes } from "./changes";
import { terminalMatches } from "./background";
import { hasPendingImage, insertPendingImage, finishPendingImage } from "./images";
import { findMatches } from "./transcript";

const row = (text: string): Row => ({ id: "answer", kind: "assistant", text, order: [1, 0] });
const page = (start = 0): RowPage => ({ page: { start_offset: start, end_offset: 20 }, rows: [row("hello")], usage: null });
const patch = "@@ -1 +1 @@\n-old\n+new";
const file = { path: "app.ts", status: "M", layer: "unstaged" as const, added: 1, removed: 1 };
const list = { files: [file], refs: ["refs/heads/main"], comparison: [], identity: { head: "a", branch: "feature" } };
const deferred = <T,>() => { let resolve!: (value: T) => void; const promise = new Promise<T>((r) => { resolve = r; }); return { promise, resolve }; };
const tick = async () => { await new Promise((resolve) => setTimeout(resolve, 5)); };

beforeEach(() => {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k) });
  vi.stubGlobal("requestAnimationFrame", () => 0);
  vi.stubGlobal("document", { activeElement: null });
});
afterEach(() => vi.unstubAllGlobals());

function changeHarness(reply?: (url: string) => Promise<Response>) {
  const callbacks: (() => void)[] = [];
  const api = { headers: () => ({}), fetch: (url: string) => reply?.(url) ?? Promise.resolve(Response.json(url.startsWith("/api/changes/diff") ? { ...file, patch } : url.startsWith("/api/pr") ? { pullRequest: null, checks: [] } : list)) };
  const app = { view: { workspace: "A", openId: "chat" }, api, subscribe: (fn: () => void) => callbacks.push(fn), chair: () => ({ disabled: null }), send: async () => true, notice: () => {} };
  const changes = new Changes(app as unknown as App);
  return { app, changes, workspace: (id: string) => { app.view.workspace = id; callbacks.forEach((fn) => fn()); } };
}

describe("chat UX regressions", () => {
  it("keeps a pending image at its marker and preserves edits made during upload", () => {
    const marker = "[[Uploading image test]]";
    const pending = insertPendingImage("old selection tail", 0, 13, marker);
    expect(hasPendingImage(pending)).toBe(true);
    const edited = `new text ${pending}`;
    expect(finishPendingImage(edited, marker, "/uploads/ready.png")).toBe(`new text  /uploads/ready.png  tail`);
    expect(finishPendingImage("owner removed it", marker, "/uploads/ready.png")).toBe("owner removed it");
  });

  it("counts each occurrence in one message", () => {
    const matches = findMatches([row("term then TERM")], "term");
    expect(matches.map((m) => m.offset)).toEqual([0, 10]);
    expect(matches.map((m) => m.occurrence)).toEqual([0, 1]);
  });

  it("finds ready full output at its preview and line", () => {
    const output = { id: "output-1", text: "preview", full_lines: 900, full_bytes: 10000, hidden_lines: 899 };
    const r = { ...row("result"), kind: "toolResult" as const, output };
    const full = new Map([[output.id, { state: "ready", text: "preview\nunique failure twice failure" }]]);
    expect(findMatches([r], "failure", full).map((m) => [m.preview, m.line, m.lineOccurrence])).toEqual([["output-1", 1, 0], ["output-1", 1, 1]]);
  });

  it("announces an older read immediately, shows failure, and clears it after Retry", async () => {
    const slow = deferred<RowPage>();
    let reads = 0;
    const app = new App({ rows: () => ++reads === 1 ? slow.promise : Promise.resolve(page()) } as unknown as Api);
    app.openId = "chat";
    const chat = app.reading.newChat("chat");
    Object.assign(chat, { state: "ready", older: 10 });
    let noticed = false;
    app.subscribe(() => { if (chat.loadingOlder) noticed = true; });
    const loading = app.loadOlder();
    expect(noticed).toBe(true);
    slow.resolve(page());
    await loading;
    chat.older = 10;
    app.api.rows = async () => { throw new Error("offline"); };
    await app.loadOlder();
    expect(app.view.chatError).toContain("offline");
    app.api.rows = async () => page();
    await app.loadOlder();
    expect(app.view.chatError).toBeNull();
  });

  it("retries the failed transcript read on its reader", async () => {
    const app = new App({ rows: async () => page() } as unknown as Api);
    app.openId = "chat";
    const chat = app.reading.newChat("chat");
    Object.assign(chat, { state: "error", error: "offline" });
    await app.reading.retryRows("chat");
    expect(app.view.chatState).toBe("ready");
    expect(app.view.chatError).toBeNull();
  });

  it("keeps comments, panel choice and branch base across a new Changes instance", async () => {
    const { changes, app } = changeHarness();
    changes.show("changes");
    await tick();
    changes.setBase("refs/heads/main");
    changes.addComment(2, 2, "Keep this draft");
    changes.show("pr");
    const restored = new Changes(app as unknown as App);
    restored.show("pr");
    await tick();
    expect(restored.view.tab).toBe("pr");
    expect(restored.view.open).toBe(true);
    expect(restored.view.base).toBe("refs/heads/main");
    expect(restored.view.comments.map((c) => c.text)).toEqual(["Keep this draft"]);
  });

  it("rejects a PR reply from the workspace that was left", async () => {
    const oldPr = deferred<Response>();
    const { changes, workspace } = changeHarness((url) => url.startsWith("/api/pr") ? oldPr.promise : Promise.resolve(Response.json(list)));
    changes.show("pr");
    changes.show("changes");
    workspace("B");
    oldPr.resolve(Response.json({ pullRequest: { title: "A only" }, checks: [] }));
    await tick();
    expect(changes.view.workspace).toBe("B");
    expect(changes.view.pr.state).toBe("idle");
  });

  it("keeps a comment editor per file and clears only explicit Cancel", () => {
    const { changes, workspace } = changeHarness();
    changes.show("changes");
    const editor = { anchor: 1, cursor: 2, text: "Before Add" };
    changes.setEditor("app.ts", "unstaged", editor);
    changes.setEditor("other.ts", "unstaged", { ...editor, text: "Other" });
    expect(changes.editor("app.ts", "unstaged")).toEqual(editor);
    workspace("B");
    expect(changes.editor("app.ts", "unstaged")).toBeNull();
    workspace("A");
    expect(changes.editor("app.ts", "unstaged")).toEqual(editor);
    changes.setEditor("app.ts", "unstaged", null);
    expect(changes.editor("app.ts", "unstaged")).toBeNull();
  });

  it("moves the zoom target and subscriptions with keyboard pane focus", () => {
    const app = new App({} as Api);
    app.openId = "chat";
    app.pane = { focused: "first", zoomed: "first", reveal: null, revealSeq: 0 };
    const sync = vi.spyOn(app.streaming, "syncSubscriptions");
    // makeView normally prunes panes absent from the snapshot; inspect state before that render.
    vi.spyOn(app, "notify").mockImplementation(() => {});
    app.focusPane("second");
    expect(app.pane.zoomed).toBe("second");
    expect(sync).toHaveBeenCalled();
  });

  it("finds terminal text in its buffer, including repeated words", () => {
    expect(terminalMatches(["build ready", "ERROR then error"], "error")).toEqual([{ line: 1, column: 0 }, { line: 1, column: 11 }]);
    expect(terminalMatches(["build"], "")).toEqual([]);
  });
});
