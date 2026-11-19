import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { stripVTControlCharacters } from "node:util";
import type * as Pi from "@earendil-works/pi-coding-agent";

// Run with PI_SDK=/path/to/pi-coding-agent/dist/index.js and Node 24 or later.
const sdk = pathToFileURL(resolve(process.env.PI_SDK ?? ""));
const { DefaultResourceLoader, SettingsManager, initTheme }: typeof Pi = await import(sdk.href);
const dir = mkdtempSync(join(tmpdir(), "swarm-pi-questions-"));
const loader = new DefaultResourceLoader({
  cwd: dir,
  agentDir: dir,
  settingsManager: SettingsManager.inMemory(),
  additionalExtensionPaths: [fileURLToPath(new URL("../src/bus/pi_live.ts", import.meta.url))],
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

type Dialogs = Pick<Pi.ExtensionUIContext, "select" | "input">;

async function ask(name: string, params: unknown, ui: Partial<Dialogs>) {
  const entry = tools.find(({ definition }) => definition.name === name);
  assert.ok(entry, `Missing Pi tool: ${name}`);
  return entry.definition.execute("test-call", params, undefined, undefined, {
    hasUI: true,
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
  const { InteractiveMode } = await import(new URL("modes/interactive/interactive-mode.js", sdk).href);
  initTheme("dark");
  const plan = "1. Read the README.\n2. Report the title.";
  for (const [decision, feedback] of [["approve"], ["review", "Check LICENSE too"], ["review"], ["cancel"]]) {
    const result = await ask("approve_plan", { plan }, {
      select: (...args) => InteractiveMode.prototype.showExtensionSelector.call({
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
