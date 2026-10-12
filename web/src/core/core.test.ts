import { describe, expect, it } from "vitest";
import { takeToken, type Api, type Snapshot } from "./api";
import { App } from "./app";
import { eventKey, parseChord, Registry, search } from "./commands";
import { renderMarkdown } from "./markdown";
import { MAX_RECORDS, measure } from "./perf";
import { SseParser } from "./sse";
import { blocks, items, preview, Transcript } from "./transcript";
import { buildTree, flatten, rollUp } from "./tree";

const meta = (uuid: string) => ({ session_id: "s1", uuid, timestamp: "", parent_uuid: null, source_tool_use_id: null });

describe("transcript model", () => {
  it("merges agent chunks that share a uuid into one row", () => {
    const t = new Transcript();
    t.apply([
      { type: "agent_message_chunk", text: "Hello ", meta: meta("a-1") },
      { type: "session_info", kind: "model", value: "x", meta: meta("a-1") },
      { type: "agent_message_chunk", text: "world", meta: meta("a-1") },
    ]);
    t.apply([{ type: "agent_message_chunk", text: "!", meta: meta("a-1") }]);
    expect(t.rows).toEqual([{ id: "a:a-1", kind: "agent", text: "Hello world!" }]);
  });

  it("keeps rows when the batch ends on an event that makes no row", () => {
    const t = new Transcript();
    expect(t.apply([{ type: "agent_message_chunk", text: "done", meta: meta("a-9") }, { type: "turn_ended", meta: meta("") }])).toBe(true);
    expect(t.rows).toHaveLength(1);
    expect(t.apply([{ type: "turn_ended", meta: meta("") }])).toBe(false);
  });

  it("keeps chunks with an empty uuid as separate rows", () => {
    const t = new Transcript();
    t.apply([
      { type: "agent_message_chunk", text: "one", meta: meta("") },
      { type: "agent_message_chunk", text: "two", meta: meta("") },
    ]);
    expect(t.rows.map((r) => r.kind === "agent" && r.text)).toEqual(["one", "two"]);
  });

  it("joins a tool call with its update and diff by tool_call_id, and keeps the row id", () => {
    const t = new Transcript();
    t.apply([{ type: "tool_call", tool_call_id: "toolu-build", name: "Bash", input: { command: "make" }, status: "pending", meta: meta("a-2") }]);
    const first = t.rows[0];
    t.apply([
      { type: "agent_message_chunk", text: "while waiting", meta: meta("a-3") },
      { type: "tool_call_update", tool_call_id: "toolu-build", status: "failed", content: "Exit code 1", meta: meta("u-1") },
      { type: "tool_diff", tool_call_id: "toolu-build", path: "/w/a.txt", hunks: [], meta: meta("u-1") },
    ]);
    expect(t.rows).toHaveLength(2);
    expect(t.rows[0]).toMatchObject({ id: first.id, kind: "tool", name: "Bash", status: "failed", output: "Exit code 1" });
    expect(t.rows[0]).not.toBe(first);
    expect(t.rows[0].kind === "tool" && t.rows[0].diffs[0].path).toBe("/w/a.txt");
  });

  it("keeps interleaved calls apart and shows an update without a call as its own row", () => {
    const t = new Transcript();
    t.apply([
      { type: "tool_call", tool_call_id: "a", name: "Read", input: {}, status: "pending", meta: meta("1") },
      { type: "tool_call", tool_call_id: "b", name: "Bash", input: {}, status: "pending", meta: meta("2") },
      { type: "tool_call_update", tool_call_id: "b", status: "completed", content: "B", meta: meta("3") },
      { type: "tool_call_update", tool_call_id: "a", status: "completed", content: "A", meta: meta("4") },
      { type: "tool_call_update", tool_call_id: "lost", status: "completed", content: "L", meta: meta("5") },
    ]);
    expect(t.rows.map((r) => r.kind === "tool" && [r.name, r.output])).toEqual([
      ["Read", "A"],
      ["Bash", "B"],
      ["", "L"],
    ]);
  });

  it("merges an older page into rows that are split across the page edge", () => {
    const t = new Transcript();
    t.apply([
      { type: "tool_call_update", tool_call_id: "x", status: "completed", content: "out", meta: meta("2") },
      { type: "agent_message_chunk", text: "tail", meta: meta("m") },
    ]);
    const ids = t.rows.map((r) => r.id);
    t.prepend(
      [
        { type: "user_message_chunk", text: "hi", meta: meta("u") },
        { type: "agent_message_chunk", text: "head ", meta: meta("m0") },
        { type: "tool_call", tool_call_id: "x", name: "Bash", input: { command: "ls" }, status: "pending", meta: meta("1") },
      ],
      { start_offset: 0, end_offset: 100 },
    );
    expect(t.rows.map((r) => r.id).slice(-2)).toEqual(ids);
    expect(t.rows).toHaveLength(4);
    expect(t.get("t:x")).toMatchObject({ name: "Bash", output: "out" });
    expect(t.page.start_offset).toBe(0);
  });

  it("caps the preview at 120 lines and 12,000 characters", () => {
    const lines = Array.from({ length: 200 }, (_, i) => `line ${i}`).join("\n");
    const p = preview(lines);
    expect(p.truncated).toBe(true);
    expect(p.text.split("\n").filter(Boolean)).toHaveLength(120);
    const wide = "x".repeat(20_000);
    expect(preview(wide).text).toHaveLength(12_000);
    expect(preview("short\n")).toEqual({ text: "short\n", truncated: false });
  });

  it("splits a 200k-line output into many small blocks that rebuild the text", () => {
    const text = Array.from({ length: 200_000 }, (_, i) => `row ${i}`).join("\n");
    const b = blocks("t:x", text);
    expect(b.length).toBe(1000);
    expect(b.map((x) => x.text).join("")).toBe(text);
    const list = items([{ id: "t:x", kind: "tool", name: "", input: null, status: "completed", output: text, diffs: [] }], new Set(["t:x"]), new Map());
    expect(list).toHaveLength(1001);
  });
});

describe("sidebar tree", () => {
  const session = (id: string, workspace: string, project: string | null, created: number) => ({
    id,
    cwd: workspace,
    workspace,
    project,
    created_at: created,
    chair_provider: "claude",
    adapter: null,
  });
  const snap: Snapshot = {
    version: 1,
    sessions: [
      session("chat-old", "/r/app", "/r/app", 1),
      session("chat-new", "/r/app", "/r/app", 2),
      session("chat-wt", "/r/wt/feature", "/r/app", 3),
      session("chat-lone", "/tmp/x", null, 4),
    ],
    agents: {
      "chat-old": [{ id: "w1", role: "worker", state: "working", alive: true }],
      "chat-wt": [
        { id: "w2", role: "worker", state: "waiting", alive: true },
        { id: "w3", role: "worker", state: "done", alive: false },
      ],
    },
  };

  it("groups chats under workspace under project in a stable order", () => {
    const rows = flatten(buildTree(snap), new Set());
    expect(rows.map((r) => `${r.depth}:${r.id}`)).toEqual([
      "1:p:/r/app",
      "2:w:/r/app",
      "3:c:chat-new",
      "3:c:chat-old",
      "2:w:/r/wt/feature",
      "3:c:chat-wt",
      "1:p:/tmp/x",
      "2:w:/tmp/x",
      "3:c:chat-lone",
    ]);
  });

  it("rolls agent state up: waiting beats working beats idle", () => {
    const rows = flatten(buildTree(snap), new Set());
    const state = (id: string) => rows.find((r) => r.id === id)!.state;
    expect(state("c:chat-old")).toBe("working");
    expect(state("c:chat-wt")).toBe("waiting");
    expect(state("w:/r/app")).toBe("working");
    expect(state("p:/r/app")).toBe("waiting");
    expect(state("p:/tmp/x")).toBe("idle");
    expect(rollUp([])).toBe("idle");
  });

  it("hides the children of a collapsed row", () => {
    const rows = flatten(buildTree(snap), new Set(["p:/r/app"]));
    expect(rows.map((r) => r.id)).toEqual(["p:/r/app", "p:/tmp/x", "w:/tmp/x", "c:chat-lone"]);
    expect(rows[0].expanded).toBe(false);
  });
});

describe("command registry", () => {
  const key = (k: Partial<KeyboardEvent>) => ({ key: "", code: "", ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...k });

  it("matches a chord by physical key and exact modifiers", () => {
    const hits: string[] = [];
    const r = new Registry();
    r.add(
      { id: "palette", title: "Palette", chord: "Meta+K", run: () => hits.push("palette") },
      { id: "next", title: "Next chat", chord: "Shift+Meta+]", run: () => hits.push("next") },
      { id: "ws", title: "Next workspace", chord: "Ctrl+Meta+ArrowDown", run: () => hits.push("ws") },
      { id: "down", title: "Down", chord: "j", when: () => false, run: () => hits.push("down") },
    );
    expect(r.match(key({ key: "k", code: "KeyK", metaKey: true }))?.id).toBe("palette");
    expect(r.match(key({ key: "k", code: "KeyK", metaKey: true, shiftKey: true }))).toBeUndefined();
    expect(r.match(key({ key: "}", code: "BracketRight", metaKey: true, shiftKey: true }))?.id).toBe("next");
    expect(r.match(key({ key: "ArrowDown", code: "ArrowDown", metaKey: true, ctrlKey: true }))?.id).toBe("ws");
    expect(r.match(key({ key: "j", code: "KeyJ" }))).toBeUndefined();
  });

  it("parses chords and reads keys", () => {
    expect(parseChord("Ctrl+Meta+ArrowUp")).toEqual({ ctrl: true, meta: true, shift: false, alt: false, key: "arrowup" });
    expect(eventKey(key({ key: "¡", code: "Digit1" }))).toBe("1");
  });

  it("palette search needs every word to match", () => {
    const all = [{ title: "app / feature / claude 1" }, { title: "app / main / codex 2" }];
    expect(search(all, "app claude")).toEqual([all[0]]);
    expect(search(all, "")).toEqual(all);
  });
});

describe("SSE parser", () => {
  it("handles frames split across chunks, CRLF, comments and multi-line data", () => {
    const p = new SseParser();
    expect(p.push("event: hel")).toEqual([]);
    expect(p.push('lo\ndata: {"stream"')).toEqual([]);
    expect(p.push(':"s1"}\n\n: ping comment\n\nevent: transcript\r\ndata: line one\r\ndata: line two\r\n\r\ndata:x\n\n')).toEqual([
      { event: "hello", data: '{"stream":"s1"}' },
      { event: "transcript", data: "line one\nline two" },
      { event: "message", data: "x" },
    ]);
  });
});

describe("token", () => {
  it("reads the token from the hash and removes it from the URL", () => {
    const calls: unknown[][] = [];
    const hist = { state: null, replaceState: (...a: unknown[]) => calls.push(a) };
    const token = takeToken({ hash: "#token=abc123", pathname: "/solid.html", search: "?live=all" }, hist as unknown as History);
    expect(token).toBe("abc123");
    expect(calls).toEqual([[null, "", "/solid.html?live=all"]]);
  });

  it("returns null and leaves the URL when there is no token", () => {
    const calls: unknown[] = [];
    const hist = { state: null, replaceState: (...a: unknown[]) => calls.push(a) };
    expect(takeToken({ hash: "", pathname: "/", search: "" }, hist as unknown as History)).toBeNull();
    expect(calls).toEqual([]);
  });
});

describe("markdown", () => {
  it("does not render raw HTML or script", () => {
    const html = renderMarkdown('hi <script>alert(1)</script> <img src=x onerror="alert(1)">');
    expect(html).not.toContain("<script");
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;script&gt;");
  });

  it("links only http(s) destinations", () => {
    const html = renderMarkdown("[a](javascript:alert(1)) [b](data:text/html,x) [c](https://example.com) [d](ftp://x) [e](/local)");
    expect(html.match(/href=/g)).toHaveLength(1);
    expect(html).toContain('<a href="https://example.com" target="_blank" rel="noopener noreferrer">c</a>');
  });

  it("does not load images", () => {
    expect(renderMarkdown("![x](https://example.com/a.png)")).not.toContain("<img");
  });
});

describe("chat LRU", () => {
  it("drops the render caches and older pages of the least recent chat when a fifth chat opens", async () => {
    const fetched: string[] = [];
    const api = {
      transcript: async (session: string, _agent: string, _tail: number, before?: number) => {
        fetched.push(before === undefined ? `${session} tail` : `${session} before ${before}`);
        if (before !== undefined) return { page: { start_offset: 0, end_offset: 100 }, events: [{ type: "agent_message_chunk", text: "old", meta: meta(`${session}-old`) }] };
        return {
          page: { start_offset: 100, end_offset: 200 },
          events: [
            { type: "tool_call", tool_call_id: `${session}-cat`, name: "Bash", input: {}, status: "pending", meta: meta(`${session}-call`) },
            { type: "tool_call_update", tool_call_id: `${session}-cat`, status: "completed", content: "output", meta: meta(`${session}-result`) },
          ],
        };
      },
    } as unknown as Api;
    const settle = () => new Promise((r) => setTimeout(r, 0));
    const app = new App(api);
    app.openChat("first");
    await settle();
    await app.loadOlder();
    app.toggleOutput("t:first-cat");
    const firstMarkdown = app.view.md;
    expect(app.view.items.map((i) => i.kind)).toEqual(["agent", "tool", "block"]);

    for (const id of ["second", "third", "fourth", "fifth"]) {
      app.openChat(id);
      await settle();
    }
    app.openChat("first");
    await settle();

    expect(fetched.filter((f) => f.startsWith("first"))).toEqual(["first tail", "first before 100", "first tail"]);
    expect(app.view.items.map((i) => i.kind)).toEqual(["tool"]);
    expect(app.view.expanded.size).toBe(0);
    expect(app.view.md).not.toBe(firstMarkdown);
  });
});

describe("perf records", () => {
  it("keeps only the newest records", () => {
    const g = globalThis as unknown as { window?: unknown; requestAnimationFrame?: unknown };
    g.window = { __swarmPerf: [] };
    g.requestAnimationFrame = (fn: () => void) => fn();
    try {
      for (let i = 0; i < MAX_RECORDS + 5; i++) measure("key", `row-${i}`, i, () => true);
      const records = (g.window as Window).__swarmPerf;
      expect(records).toHaveLength(MAX_RECORDS);
      expect(records[0].target).toBe("row-5");
      expect(records.at(-1)!.target).toBe(`row-${MAX_RECORDS + 4}`);
    } finally {
      delete g.window;
      delete g.requestAnimationFrame;
    }
  });
});
