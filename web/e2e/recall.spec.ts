import { expect, test } from "@playwright/test";
import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";

// The composer's recall on the live git chat from serve.ts: Up and Down history, the Esc stash,
// the @ file menu, ⌘K Prompts, Reuse, the expand control, and the jump keys.

const typed = () => {
  try {
    return readFileSync(join(process.env.SWARM_E2E_HOME!, "typed"), "utf8");
  } catch {
    return "";
  }
};

test("the composer recalls, stashes, reuses and finds prompts, and @ inserts a file", async ({ page: tab }, info) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_GIT}"]`).click();
  const composer = tab.getByTestId("composer");
  const status = tab.getByTestId("composer-status");
  await expect(composer).toBeEnabled();
  await composer.click();

  // A sent prompt comes back with Up; Down past it gives the typed draft back.
  const prompt = `recall me from ${info.project.name}`;
  await tab.keyboard.type(prompt);
  await tab.keyboard.press("Enter");
  await expect.poll(typed).toContain(prompt);
  await expect(composer).toHaveValue("");
  await tab.keyboard.press("ArrowUp");
  await expect(composer).toHaveValue(prompt);
  await tab.keyboard.press("ArrowDown");
  await expect(composer).toHaveValue("");

  // Esc keeps the cleared draft, and Esc in the empty box gives it back.
  await tab.keyboard.type("half a thought");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("");
  await expect(status).toContainText("Draft cleared. Esc brings it back.");
  await expect(status).toHaveAttribute("role", "status");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("half a thought");
  await tab.keyboard.press("Escape");
  // Esc clears a box of spaces and keeps the stash.
  await tab.keyboard.type("   ");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("half a thought");
  await tab.keyboard.press("Escape");

  // Browsing that ends without Down keeps the typed draft where Esc finds it: after Esc on the
  // recalled prompt, and after an edit of it, also on a second browse.
  const fromStart = () => composer.evaluate((el: HTMLTextAreaElement) => el.setSelectionRange(0, 0));
  await tab.keyboard.type("my draft");
  await fromStart(); // WebKit on macOS does not move the caret on Home.
  await tab.keyboard.press("ArrowUp");
  await expect(composer).toHaveValue(prompt);
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("my draft");
  for (const draft of ["my draft", "second draft"]) {
    await composer.fill(draft);
    await fromStart();
    await tab.keyboard.press("ArrowUp");
    await expect(composer).toHaveValue(prompt);
    await tab.keyboard.type("x");
    await composer.fill("");
    await tab.keyboard.press("Escape");
    await expect(composer).toHaveValue(draft);
  }
  // Down past the newest gives the draft back, so the stash keeps what it had.
  await tab.keyboard.press("Escape");
  await composer.fill("third draft");
  await fromStart();
  await tab.keyboard.press("ArrowUp");
  await tab.keyboard.press("ArrowDown");
  await expect(composer).toHaveValue("third draft");
  await composer.fill("");
  await expect(status).toContainText("Draft cleared. Esc brings it back.");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("second draft");
  await tab.keyboard.press("Escape");
  // Esc on a prompt recalled from an empty box keeps the stash as it was.
  await tab.keyboard.press("ArrowUp");
  await expect(composer).toHaveValue(prompt);
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("second draft");
  await tab.keyboard.press("Escape");

  // Down onto a recalled prompt that ends in an @path opens no menu, so Enter sends it.
  await tab.keyboard.type("check @README.md");
  await tab.keyboard.press("Escape");
  await tab.keyboard.press("Enter");
  await expect.poll(typed).toContain("check @README.md");
  await tab.keyboard.press("ArrowUp");
  await tab.keyboard.press("ArrowUp");
  await expect(composer).toHaveValue(prompt);
  await tab.keyboard.press("ArrowDown");
  await expect(composer).toHaveValue("check @README.md");
  await expect(tab.getByTestId("slash-menu")).toBeHidden();
  await composer.fill("");

  // @ lists the chat folder's files, and Tab inserts the path with a space.
  await tab.keyboard.type("see @READ");
  const menu = tab.getByTestId("slash-menu");
  await expect(menu).toHaveAttribute("aria-label", "Files");
  await expect(menu.getByTestId("slash-row").first()).toContainText("@README.md");
  await tab.keyboard.press("Tab");
  await expect(composer).toHaveValue("see @README.md ");
  await tab.keyboard.type("@module");
  await expect(menu.getByTestId("slash-row").first()).toContainText("module.ts");
  expect(await menu.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
  await tab.keyboard.press("Escape");
  await tab.keyboard.press("Escape");

  // ⌘K finds the prompt, and picking it fills the composer.
  await tab.keyboard.press("Meta+k");
  await tab.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill(`recall me from ${info.project.name}`);
  await tab.getByRole("group", { name: "Prompts" }).getByRole("option").first().click();
  await expect(composer).toHaveValue(prompt);
  await expect(composer).toBeFocused();
  await tab.keyboard.press("Escape");

  // Reuse on a sent row fills the composer and stashes the draft. The chat's log gets the prompt
  // as the CLI would write it.
  const logged = `logged prompt from ${info.project.name}`;
  const at = new Date().toISOString();
  appendFileSync(process.env.SWARM_E2E_GIT_LOG!, JSON.stringify({ type: "user", uuid: `u-${info.project.name}`, parentUuid: null, sessionId: "fixture-session", timestamp: at, message: { role: "user", content: logged } }) + "\n");
  const userRow = tab.locator('[data-testid="row"][data-kind="user"]').filter({ hasText: logged });
  await expect(userRow).toBeVisible();
  await tab.keyboard.type("keep this draft");
  await tab.keyboard.press("Escape");
  await expect(status).toContainText("Draft cleared. Esc brings it back.");
  await userRow.hover();
  await userRow.getByTestId("reuse-prompt").click();
  await expect(composer).toHaveValue(logged);
  await expect(composer).toBeFocused();
  await expect(status).not.toContainText("Draft cleared.");
  await tab.keyboard.press("Meta+a");
  await tab.keyboard.press("Backspace");
  await tab.keyboard.press("Escape");
  await expect(composer).toHaveValue("keep this draft");
  await tab.keyboard.press("Escape");

  // Reuse, or a switch to another chat, while a recalled prompt shows keeps the typed draft.
  for (const leave of ["reuse", "switch"]) {
    await composer.fill(`draft before ${leave}`);
    await composer.evaluate((el: HTMLTextAreaElement) => el.setSelectionRange(0, 0));
    await tab.keyboard.press("ArrowUp");
    await expect(composer).not.toHaveValue(`draft before ${leave}`);
    if (leave === "reuse") {
      await userRow.hover();
      await userRow.getByTestId("reuse-prompt").click();
      await expect(composer).toHaveValue(logged);
    } else {
      await tab.locator('[data-testid="tree-row"][data-kind="chat"]').filter({ hasNot: tab.locator(`[data-session="${process.env.SWARM_E2E_GIT}"]`) }).first().click();
      await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_GIT}"]`).click();
      await expect(composer).toBeEnabled();
    }
    await composer.fill("");
    await composer.press("Escape");
    await expect(composer).toHaveValue(`draft before ${leave}`);
    await composer.press("Escape");
  }

  // Expand lifts the 200 px cap.
  await expect(tab.getByTestId("composer-expand")).not.toHaveAttribute("aria-pressed");
  await tab.getByTestId("composer-expand").click();
  expect((await composer.boundingBox())!.height).toBeGreaterThan(300);
  await tab.getByRole("button", { name: "Shrink composer" }).click();
  expect((await composer.boundingBox())!.height).toBeLessThanOrEqual(200);

  // ⌃⌥↑ moves the transcript back to the prompt above a long answer; ⌃⌘W says when no chat waits.
  const answer = Array.from({ length: 80 }, (_, i) => `answer line ${i}`).join("\n\n");
  appendFileSync(process.env.SWARM_E2E_GIT_LOG!, JSON.stringify({ type: "assistant", uuid: `a-${info.project.name}`, parentUuid: `u-${info.project.name}`, sessionId: "fixture-session", timestamp: at, message: { role: "assistant", model: "claude-opus-4-6", content: [{ type: "text", text: answer }] } }) + "\n");
  await expect(tab.getByText("answer line 79")).toBeAttached();
  // The list measures the new rows after they paint, so scroll again until the end holds.
  await expect(async () => {
    await tab.getByTestId("transcript").evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(tab.getByText("answer line 79")).toBeInViewport({ timeout: 500 });
    await expect(userRow).not.toBeInViewport({ timeout: 500 });
  }).toPass();
  await tab.keyboard.press("Control+Alt+ArrowUp");
  await expect(userRow).toBeInViewport();
  await tab.keyboard.press("Control+Meta+w");
  await expect(tab.getByText("No chat is waiting for you.")).toBeVisible();
  expect(errors).toEqual([]);
});

test("a chat with no text box offers no Reuse", async ({ page: tab }) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  // ws-build's chair has no pane, so the chat shows Continue chat in place of the composer.
  const ws = tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: /^ws-build/ });
  const session = (await ws.locator("xpath=following-sibling::*[@data-kind='chat']").first().getAttribute("data-session"))!;
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.getByTestId("continue-chat")).toBeVisible();
  const userRow = tab.locator('[data-testid="row"][data-kind="user"]').first();
  await expect(userRow).toBeVisible();
  await userRow.hover();
  await expect(tab.getByTestId("reuse-prompt")).toHaveCount(0);
});
