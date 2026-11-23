import { test } from "node:test";
import assert from "node:assert/strict";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { stripVTControlCharacters } from "node:util";
import type * as Pi from "@earendil-works/pi-coding-agent";

// Run with PI_SDK_DIR=/path/to/@earendil-works/pi-coding-agent (the package folder) and Node 24 or later.
// Nothing in CI runs it; the Pi SDK is not installed there.
const sdk = pathToFileURL(resolve(process.env.PI_SDK_DIR ?? "", "dist/index.js"));
const { DefaultResourceLoader, SettingsManager, InteractiveMode, initTheme }: typeof Pi = await import(sdk.href);
const nativeSelect = Reflect.get(InteractiveMode.prototype, "showExtensionSelector");
const nativeCustom = Reflect.get(InteractiveMode.prototype, "showExtensionCustom");
const dir = mkdtempSync(join(tmpdir(), "swarm-pi-questions-"));
// Launch installs the extension beside its subagent module under these names (src/bus.rs).
const source = (name: string) => fileURLToPath(new URL(`../src/bus/${name}`, import.meta.url));
copyFileSync(source("pi_live.ts"), join(dir, "swarm-live.ts"));
copyFileSync(source("pi_subagents.ts"), join(dir, "swarm-subagents.ts"));
const loader = new DefaultResourceLoader({
  cwd: dir,
  agentDir: dir,
  settingsManager: SettingsManager.inMemory(),
  additionalExtensionPaths: [join(dir, "swarm-live.ts")],
  noExtensions: true,
  noSkills: true,
  noPromptTemplates: true,
  noThemes: true,
  noContextFiles: true,
});
try {
  await loader.reload();
} finally {
  rmSync(dir, { recursive: true, force: true });
}
const { extensions, errors } = loader.getExtensions();
assert.deepEqual(errors, []);
const tools = extensions.flatMap(({ tools }) => [...tools.values()]);

type Dialogs = Pick<Pi.ExtensionUIContext, "select" | "input" | "custom">;

async function ask(name: string, params: unknown, ui: Partial<Dialogs>) {
  const entry = tools.find(({ definition }) => definition.name === name);
  assert.ok(entry, `Missing Pi tool: ${name}`);
  return entry.definition.execute("test-call", params, undefined, undefined, {
    hasUI: true,
    mode: "tui",
    ui,
  } as Pi.ExtensionToolContext);
}

test("a Pi choice question returns the option selected by the owner", async () => {
  const result = await ask("ask_user_question", { question: "Pick a color.", choices: ["Amber", "Blue"] }, {
    async select(question, choices) {
      assert.equal(question.trim(), "Pick a color.");
      assert.deepEqual(choices, ["Amber", "Blue"]);
      return choices[1];
    },
  });
  const reply = result.content[0];
  assert.ok(reply?.type === "text" && reply.text.includes("Blue"));
});

test("a Pi text question returns the response typed by the owner", async () => {
  const result = await ask("ask_user_question", { question: "What label should I use?" }, {
    async input(question) {
      assert.equal(question, "What label should I use?");
      return "Release notes";
    },
  });
  const reply = result.content[0];
  assert.ok(reply?.type === "text" && reply.text.includes("Release notes"));
});

test("Pi renders the full plan and returns the owner's decision and optional changes", async () => {
  initTheme("dark");
  const plan = "1. Read the README.\n2. Report the title.";
  for (const [decision, feedback] of [["approve"], ["review", "Check LICENSE too"], ["review"], ["cancel"]]) {
    const result = await ask("approve_plan", { plan }, {
      select: (...args) => nativeSelect.call({
        pushExtensionDialog(dialog: { render(width: number): string[]; handleInput(key: string): void }) {
          const rendered = dialog.render(80).map((row) => stripVTControlCharacters(row).trim()).join("\n");
          assert.ok(rendered.includes(plan), rendered);
          assert.ok(rendered.includes("Request changes") && rendered.includes("Approve"), rendered);
          if (decision === "approve") dialog.handleInput("\x1b[B");
          dialog.handleInput(decision === "cancel" ? "\x1b" : "\r");
        },
        removeExtensionDialog() {},
      }, ...args),
      async input() {
        assert.equal(decision, "review");
        return feedback;
      },
    });
    assert.equal(result.details.approved, decision === "approve");
    if (feedback) {
      const reply = result.content[0];
      assert.ok(reply?.type === "text" && reply.text.includes(feedback));
    }
  }
});

test("the installed Pi renderer keeps decisions visible while paging through a tall plan", async (t) => {
  const { TuiMainScreen, getKeybindings } = await import(new URL("../../pi-tui/dist/index.js", sdk).href);
  initTheme("dark");
  const terminal = { columns: 80, rows: 24, write() {}, hideCursor() {}, showCursor() {} };
  for (const key of ["rows", "columns"] as const) {
    const original = Object.getOwnPropertyDescriptor(process.stdout, key);
    Object.defineProperty(process.stdout, key, { configurable: true, get: () => terminal[key] });
    t.after(() => {
      if (original) Object.defineProperty(process.stdout, key, original);
      else Reflect.deleteProperty(process.stdout, key);
    });
  }
  const tui = new TuiMainScreen(terminal);
  let blocking: string | undefined;
  const host = {
    ui: tui,
    keybindings: getKeybindings(),
    editor: { getText: () => "", setText() {} },
    extensionOverlays: new Map(),
    nextBlockingOverlayId: 0,
    programStatus: { setBlocked(_source: string, state?: { kind: string }) { blocking = state?.kind; } },
    pushExtensionDialog(dialog: { render(width: number): string[]; handleInput(key: string): void }) {
      // The reported live failure had only the top of the inline selector visible.
      check(dialog.render(80).slice(0, terminal.rows - 2));
      dialog.handleInput("\x1b");
    },
    removeExtensionDialog() {},
  };
  const plan = Array.from({ length: 30 }, (_, i) => `Step ${i + 1}: Inspect a file.`).join("\n");
  const check = (rows: string[]) => {
    const visible = rows.map(stripVTControlCharacters).join("\n");
    assert.ok(visible.includes("Request changes") && visible.includes("Approve"), visible);
    return visible;
  };
  const viewport = () => {
    tui.renderNow();
    const frame = tui.captureRenderState();
    return check(frame.previousLines.slice(frame.previousViewportTop, frame.previousViewportTop + terminal.rows));
  };
  // A 3-row pane has no room for plan rows, but the app still reads and presses its choices.
  for (const [rows, approved] of [[24, false], [24, true], [3, true]] as const) {
    terminal.rows = rows;
    const result = await ask("approve_plan", { plan }, {
      select: (...args) => nativeSelect.call(host, ...args),
      custom: (factory, options) => nativeCustom.call(host, factory, {
        ...options,
        onHandle() {
          assert.equal(blocking, "question");
          const first = viewport();
          if (rows > 3) {
            assert.ok(first.includes("Step 1:") && !first.includes("Step 30:"), first);
            tui.handleTerminalInput("\x1b[6~"); // Page Down
            assert.ok(viewport().includes("Step 30:"));
            tui.handleTerminalInput("\x1b[5~"); // Page Up
            assert.ok(viewport().includes("Step 1:"));
          }
          if (approved) {
            tui.handleTerminalInput("\x1b[B");
            tui.handleTerminalInput("\r");
          } else {
            tui.handleTerminalInput("\x1b");
          }
        },
      }),
    });
    assert.equal(result.details.approved, approved);
    assert.equal(blocking, undefined);
    tui.renderNow();
    assert.ok(!tui.captureRenderState().previousLines.join("\n").includes("Pi plan approval"));
  }
});
