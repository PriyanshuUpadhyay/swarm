import { expect, test, type Page } from "@playwright/test";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Where the keyboard lands after a chat opens, a switch, a copy, or a close: the owner can type at
// once, and a key walk in the sidebar keeps the sidebar.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const row = (tab: Page, session: string) => tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
const ready = (tab: Page, session: string) => tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`);
const composer = (tab: Page) => tab.locator(".strip-chat .chat-input").getByTestId("composer");

// Both browsers share one server home, so each case puts back the owner choices it changed.
let choices = "";
const choicesPath = () => join(process.env.SWARM_E2E_HOME!, ".swarm/choices.json");
test.beforeEach(() => void (choices = readFileSync(choicesPath(), "utf8")));
test.afterEach(async ({ page: tab }) => {
  await tab.close();
  writeFileSync(choicesPath(), choices);
});

async function load(tab: Page) {
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
}

test("a sidebar click, a workspace click, and a tab click put the keyboard in the chat", async ({ page: tab }) => {
  await load(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  await row(tab, live).click();
  await expect(ready(tab, live)).toBeVisible();
  await expect(composer(tab)).toBeFocused();

  await tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: "ws-tabs" }).click();
  const tabs = tab.getByTestId("tab");
  await expect(tabs).toHaveCount(2);
  await expect(composer(tab)).toBeFocused();
  const other = tabs.and(tab.locator('[aria-selected="false"]'));
  const session = (await other.getAttribute("data-session"))!;
  await other.click();
  await expect(ready(tab, session)).toBeVisible();
  await expect(composer(tab)).toBeFocused();
});

test("Enter on a sidebar row opens the chat and focuses its input", async ({ page: tab }) => {
  await load(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  const git = process.env.SWARM_E2E_GIT!;
  await row(tab, git).click();
  await expect(ready(tab, git)).toBeVisible();
  // Walk the selection to the live chat by key, as an owner would.
  await row(tab, git).focus();
  for (let i = 0; i < 40 && (await row(tab, live).getAttribute("aria-selected")) !== "true"; i++) await tab.keyboard.press(i < 20 ? "ArrowDown" : "ArrowUp");
  await expect(row(tab, live)).toBeFocused();
  await tab.keyboard.press("Enter");
  await expect(ready(tab, live)).toBeVisible();
  await expect(composer(tab)).toBeFocused();
});

test("a chat picked in the palette gets the keyboard", async ({ page: tab }) => {
  await load(tab);
  const git = process.env.SWARM_E2E_GIT!;
  await row(tab, process.env.SWARM_E2E_LIVE!).click();
  await tab.keyboard.press("Meta+k");
  await tab.getByPlaceholder("Search actions, workspaces, chats, agents, and prompts").fill((await row(tab, git).innerText()).split("\n")[0].trim());
  await tab.locator(`[cmdk-item][data-value="chat:${git}"]`).click();
  await expect(ready(tab, git)).toBeVisible();
  await expect(composer(tab)).toBeFocused();
});

test("a copy, a find close, and a send leave the keyboard in the composer", async ({ page: tab }, info) => {
  await load(tab);
  const git = process.env.SWARM_E2E_GIT!;
  await row(tab, git).click();
  await expect(ready(tab, git)).toBeVisible();

  const at = new Date().toISOString();
  const id = `focus-${info.project.name}`;
  const text = Array.from({ length: 60 }, (_, i) => `${id} line ${i}`).join("\n\n");
  appendFileSync(process.env.SWARM_E2E_GIT_LOG!, JSON.stringify({ type: "assistant", uuid: id, parentUuid: null, sessionId: "fixture-session", timestamp: at, message: { role: "assistant", model: "claude-opus-4-6", content: [{ type: "text", text: `${text}\n\n\`\`\`\ncopy me\n\`\`\`` }] } }) + "\n");
  const answer = tab.locator('[data-testid="row"]').filter({ hasText: `${id} line 0` });
  await expect(answer).toBeAttached();
  const scroller = tab.locator(".strip-chat").getByTestId("transcript");

  // Copy on a code block.
  const copy = answer.locator('[data-copy="code"]');
  await copy.scrollIntoViewIfNeeded();
  await copy.click();
  await expect(composer(tab)).toBeFocused();

  // Copy a message.
  await answer.hover();
  await answer.getByRole("button", { name: "Copy message" }).click();
  await expect(composer(tab)).toBeFocused();

  // The find bar's close button.
  await tab.keyboard.press("Meta+f");
  await expect(tab.getByTestId("find-input")).toBeFocused();
  await tab.getByRole("button", { name: "Close find" }).click();
  await expect(composer(tab)).toBeFocused();

  // Escape in the find bar.
  await tab.keyboard.press("Meta+f");
  await expect(tab.getByTestId("find-input")).toBeFocused();
  await tab.keyboard.press("Escape");
  await expect(composer(tab)).toBeFocused();

  // A send while scrolled up moves the transcript to its end, where the new message lands. The
  // stand-in pane can be slow to take the text, so the end is checked, not the sent row.
  const end = answer.getByText("copy me");
  // A row that lands before the scroll event follows to the end again, so scroll until the
  // transcript reads as scrolled up.
  await expect(async () => {
    await scroller.evaluate((el) => (el.scrollTop = 0));
    await expect(tab.getByTestId("jump-latest")).toBeVisible({ timeout: 1000 });
  }).toPass();
  await expect(end).not.toBeInViewport();
  await composer(tab).fill(`sent from far up ${info.project.name}`);
  await tab.getByTestId("send").click();
  await expect(end).toBeInViewport();
  await expect(composer(tab)).toBeFocused();
});
