import { describe, expect, it } from "vitest";
import { takeToken, type Api, type Row, type RowPage, type Snapshot } from "./api";
import { App } from "./app";
import { eventKey, installKeys, parseChord, Registry } from "./commands";
import { renderMarkdown } from "./markdown";
import { paletteSections } from "./nav";
import { MAX_RECORDS, measure } from "./perf";
import { SseParser } from "./sse";
import { FoldChoices, findMatches, isPatch, lines, mergeRows, parsePatch, patchKinds, splitPatch, wordAt } from "./transcript";

// A row as the server sends it; `order` is [source offset, event ordinal].
const row = (id: string, offset: number, extra: Partial<Row> = {}): Row => ({ id, kind: "assistant", text: id, order: [offset, 0], ...extra });
const tool = (id: string, offset: number, state: "finished" | "failed" = "finished"): Row =>
  row(id, offset, { kind: "toolUse", tool: { name: "Bash", input: {}, state, header_title: id } });
const fold = (id: string, steps: Row[], extra: Partial<Row> = {}): Row =>
  row(id, steps[0].order[0], { kind: "fold", text: `${steps.length} commands`, steps, ...extra });
const ids = (rows: { id: string }[]) => rows.map((r) => r.id);

describe("row merge", () => {
  it("replaces a known row in place, appends a newer one, and puts an older one by order", () => {
    const first = [row("question", 10), row("answer", 20)];
    const merged = mergeRows(first, [row("answer", 20, { text: "longer answer" }), row("follow-up", 30), row("preamble", 5)]);
    expect(ids(merged)).toEqual(["preamble", "question", "answer", "follow-up"]);
    expect(merged[2].text).toBe("longer answer");
    expect(merged[1]).toBe(first[0]);
  });

  it("places by offset, then by event ordinal, and keeps queued rows last", () => {
    const merged = mergeRows([row("queued-prompt", 1, { kind: "queued" })], [row("second-event", 40, { order: [40, 1] }), row("first-event", 40)]);
    expect(ids(merged)).toEqual(["first-event", "second-event", "queued-prompt"]);
  });

  it("removes rows by id, and keeps a row that the same frame upserts again", () => {
    const orphanResult = row("orphan-result", 50, { kind: "toolResult" });
    const merged = mergeRows([orphanResult, row("answer", 60)], [tool("joined-call", 40), row("answer", 60, { text: "same id" })], ["orphan-result", "answer"]);
    expect(ids(merged)).toEqual(["joined-call", "answer"]);
    expect(merged[1].text).toBe("same id");
  });

  it("returns the same array when a frame changes nothing", () => {
    const rows = [row("answer", 1)];
    expect(mergeRows(rows, [], [])).toBe(rows);
  });
});

describe("folds and lines", () => {
  const steps = [tool("read-config", 100), row("model-notice", 101, { kind: "notice", collapsed: true }), tool("run-tests", 102)];

  it("draws a closed fold as one line, and an open fold with every step under it, collapsed ones too", () => {
    const state = { folds: new FoldChoices(), opened: new Map() };
    expect(ids(lines([fold("run", steps)], state))).toEqual(["run"]);
    expect(ids(lines([fold("run", steps, { default_open: true })], state))).toEqual(["run", "read-config", "model-notice", "run-tests"]);
  });

  it("shows a collapsed row as a closed line that the reader can open", () => {
    const record = row("record", 1, { kind: "system", system_kind: "record", collapsed: true });
    expect(lines([record], { folds: new FoldChoices(), opened: new Map() })[0].open).toBe(false);
    expect(lines([record], { folds: new FoldChoices(), opened: new Map([["record", true]]) })[0].open).toBe(true);
  });

  it("keeps the reader's choice when the run gains a step and the fold id changes", () => {
    const folds = new FoldChoices();
    const before = fold("fold:read-config", steps.slice(0, 2), { default_open: true });
    folds.set(before, false);
    const grown = fold("fold:read-config-v2", steps, { default_open: true });
    expect(folds.isOpen(grown)).toBe(false);
    folds.set(grown, true);
    expect(folds.isOpen(fold("fold:other", [steps[2]]))).toBe(true);
    expect(folds.isOpen(fold("fold:unseen", [tool("lint", 200)]))).toBe(false);
  });

  it("opens a failed tool card by default, and the reader's choice wins", () => {
    const failed = tool("cargo-test", 5, "failed");
    expect(lines([failed], { folds: new FoldChoices(), opened: new Map() })[0].open).toBe(true);
    expect(lines([failed], { folds: new FoldChoices(), opened: new Map([["cargo-test", false]]) })[0].open).toBe(false);
  });
});

describe("find", () => {
  it("finds a step inside a closed fold and names the fold that holds it", () => {
    const rows = [row("intro", 1, { text: "Running the parser tests" }), fold("run", [tool("read-config", 10), tool("parser-check", 11)])];
    const matches = findMatches(rows, "PARSER");
    expect(matches.map((m) => [m.id, m.fold?.id ?? null])).toEqual([
      ["intro", null],
      ["parser-check", "run"],
    ]);
  });

  it("finds collapsed rows too", () => {
    const rows = [row("model-notice", 1, { kind: "notice", text: "model: opus", collapsed: true })];
    expect(findMatches(rows, "opus")).toHaveLength(1);
  });

  it("opens the fold and the card of the match the reader steps to", async () => {
    const step = { ...tool("parser-check", 11), tool: { name: "Bash", input: {}, state: "finished" as const, header_title: "cargo test parser" } };
    const page: RowPage = { page: { start_offset: 0, end_offset: 20 }, rows: [fold("run", [tool("read-config", 10), step])], usage: null };
    const app = new App({ rows: async () => page } as unknown as Api);
    const g = globalThis as unknown as { requestAnimationFrame?: unknown; document?: unknown };
    g.requestAnimationFrame = () => 0;
    g.document = { querySelector: () => null };
    try {
      app.openChat("chat");
      await new Promise((r) => setTimeout(r, 0));
      expect(ids(app.view.lines)).toEqual(["run"]);
      app.openFind();
      app.setFindQuery("parser");
      expect(ids(app.view.lines)).toEqual(["run", "read-config", "parser-check"]);
      expect(app.view.lines[2].open).toBe(true);
      expect(app.view.find.matches[app.view.find.index].id).toBe("parser-check");
    } finally {
      delete g.requestAnimationFrame;
      delete g.document;
    }
  });
});

describe("patch", () => {
  it("numbers unified lines and pairs removals with additions side by side", () => {
    const parsed = parsePatch('diff --git "a/x" "b/x"\n--- "a/x"\n+++ "b/x"\n@@ -3,3 +3,3 @@\n keep\n-old\n+new\n tail\n');
    expect(parsed.map((l) => [l.kind, l.old ?? null, l.new ?? null])).toEqual([
      ["meta", null, null],
      ["meta", null, null],
      ["meta", null, null],
      ["hunk", null, null],
      ["ctx", 3, 3],
      ["del", 4, null],
      ["add", null, 4],
      ["ctx", 5, 5],
    ]);
    const split = splitPatch(parsed);
    expect(split[2]).toEqual({ left: parsed[5], right: parsed[6] });
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

  it("swarmCommand runs a registered command by id", () => {
    const hits: string[] = [];
    const r = new Registry();
    r.add(
      { id: "palette", title: "Palette", chord: "Meta+K", run: () => hits.push("palette") },
      { id: "down", title: "Down", chord: "j", when: () => false, run: () => hits.push("down") },
    );
    const page = { addEventListener: () => {}, removeEventListener: () => {} } as unknown as Window;
    const uninstall = installKeys(r, page);
    expect(page.swarmCommand!("palette")).toBe(true);
    expect(page.swarmCommand!("down")).toBe(false);
    expect(page.swarmCommand!("workspace.new")).toBe(false);
    expect(hits).toEqual(["palette"]);
    uninstall();
    expect(page.swarmCommand).toBeUndefined();
  });

  it("parses chords and reads keys", () => {
    expect(parseChord("Ctrl+Meta+ArrowUp")).toEqual({ ctrl: true, meta: true, shift: false, alt: false, key: "arrowup" });
    expect(eventKey(key({ key: "¡", code: "Digit1" }))).toBe("1");
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

  it("gives code blocks and tables a Copy header; a table keeps its markdown source", () => {
    const html = renderMarkdown("```sh\nmake build\n```\n\n| a | b |\n|---|---|\n| 1 | <2> |\n");
    expect(html).toContain('<div class="md-code"><div class="md-head"><span>sh</span><button type="button" data-copy="code"');
    expect(html).toContain('data-raw="| a | b |\n|---|---|\n| 1 | &lt;2&gt; |"');
    expect(html).toContain('data-copy="table"');
  });

  it("does not load images", () => {
    expect(renderMarkdown("![x](https://example.com/a.png)")).not.toContain("<img");
  });
});

describe("chat LRU", () => {
  it("drops the reader state and older pages of the least recent chat when a fifth chat opens", async () => {
    const fetched: string[] = [];
    const api = {
      rows: async (session: string, _agent: string, _tail: number, before?: number): Promise<RowPage> => {
        fetched.push(before === undefined ? `${session} tail` : `${session} before ${before}`);
        if (before !== undefined) return { page: { start_offset: 0, end_offset: 100 }, rows: [row(`${session}-old`, 50), tool(`${session}-cat`, 150, "failed")], usage: null };
        return { page: { start_offset: 100, end_offset: 200 }, rows: [tool(`${session}-cat`, 150, "failed")], usage: null };
      },
    } as unknown as Api;
    const settle = () => new Promise((r) => setTimeout(r, 0));
    const app = new App(api);
    app.openChat("first");
    await settle();
    await app.loadOlder();
    app.toggleLine(app.view.lines[1]);
    const firstMarkdown = app.view.md;
    expect(app.view.lines.map((l) => [l.id, l.open])).toEqual([
      ["first-old", false],
      ["first-cat", false],
    ]);

    for (const id of ["second", "third", "fourth", "fifth"]) {
      app.openChat(id);
      await settle();
    }
    app.openChat("first");
    await settle();

    expect(fetched.filter((f) => f.startsWith("first"))).toEqual(["first tail", "first before 100", "first tail"]);
    expect(app.view.lines.map((l) => [l.id, l.open])).toEqual([["first-cat", true]]);
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

describe("diffs in plain text", () => {
  it("colours `diff -u` output by hunk counts, so a removed `-- comment` is not a file header", () => {
    const out = ["--- a.sql\t2026", "+++ a.sql\t2026", "@@ -1,2 +1,2 @@", " keep", "--- old comment", "+-- new comment", "done"];
    expect(isPatch(out.join("\n"))).toBe(true);
    expect(patchKinds(out)).toEqual(["meta", "meta", "hunk", "ctx", "del", "add", "ctx"]);
    expect(isPatch("a + b\n- item")).toBe(false);
  });

  it("colours a diff fence and keeps its text", () => {
    const html = renderMarkdown("```diff\n-old\n+new\n```");
    expect(html).toContain('<span class="dl del">-old\n</span><span class="dl add">+new\n</span>');
  });

  it("renders a hunk fence with wrong counts and no file header in both layouts, escaped", () => {
    // The counts say 1 line, but four follow and a second hunk comes after them.
    const html = renderMarkdown("```diff\n@@ -1,1 +1,1 @@\n keep\n-<script>x</script>\n+new\n-gone\n@@ -9,1 +9,1 @@\n-late\n```");
    expect(html).toContain('<div class="d2h bare"><div class="unified">');
    expect(html).toContain('<div class="split">');
    expect(html).not.toContain("<script>");
    expect(html).toContain('data-raw="@@ -1,1 +1,1 @@\n keep\n-&lt;script&gt;');
    const unified = html.slice(0, html.indexOf('<div class="split">'));
    expect(unified.match(/<td class="d2h-del/g)).toHaveLength(3);
    expect(unified.match(/<td class="d2h-ins/g)).toHaveLength(1);
    expect(unified).toContain("@@ -9,1 +9,1 @@");
  });
});

describe("owner messages and skill chips", () => {
  it("keeps the owner's typed line breaks, and agent text stays plain markdown", () => {
    expect(renderMarkdown("one\ntwo", true)).toContain("one<br>\ntwo");
    expect(renderMarkdown("one\ntwo")).not.toContain("<br>");
  });

  it("finds a skill token only as its own word", () => {
    expect(wordAt("please use /skill:plan now", "/skill:plan")).toBe(11);
    expect(wordAt("see /skill:plans and (/skill:plan)", "/skill:plan")).toBe(-1);
    expect(wordAt("see /skill:plans and (/skill:plan\nnext", "/skill:plan")).toBe(22);
  });
});

describe("prompt history", () => {
  it("records a send for the workspace, so a new chat recalls it, and Reuse keeps the draft", async () => {
    let refuse = false;
    const api = {
      action: async () => { if (refuse) throw new Error("no pane"); },
      rows: async (): Promise<RowPage> => ({ page: { start_offset: 0, end_offset: 0 }, rows: [], usage: null }),
    } as unknown as Api;
    const app = new App(api);
    const session = (id: string, workspace: string) => ({ id, cwd: `/w/${workspace}`, created_at: 0, chair_provider: "codex", adapter: "tmux", project: "p", workspace });
    const chair = { id: "orchestrator", role: "orchestrator", state: "idle", alive: true, pane: "%1", provider: "codex" };
    (app as unknown as { applySnapshot(s: Snapshot): void }).applySnapshot({
      version: 1,
      sessions: [session("a", "main"), session("b", "main"), session("c", "other")],
      agents: { a: [chair], b: [chair], c: [chair] },
    });

    refuse = true;
    expect(await app.send("a", "not sent")).toBe(false);
    refuse = false;
    expect(await app.send("a", "  run the tests  ")).toBe(true);

    expect(app.history("b")).toEqual(["run the tests"]);
    expect(app.history("c")).toEqual([]);

    app.setDraft("b", "half typed");
    app.reuse("b", "run the tests");
    expect(app.draft("b")).toBe("run the tests");
    expect(app.stashed("b")).toBe("half typed");

    // ⌘K lists the prompts only for a query.
    app.openChat("b");
    const sections = (q: string) => paletteSections(app.paletteItems(), q).map((s) => [s.title, s.items.map((i) => i.title)]);
    expect(sections("tests")).toContainEqual(["Prompts", ["run the tests"]]);
    expect(sections("").flatMap(([, titles]) => titles)).not.toContain("run the tests");
  });
});
