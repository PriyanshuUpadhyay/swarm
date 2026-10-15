import { expect, test, type Page } from "@playwright/test";

// The URL names the open chat (ADR 0088), and the page keeps its token over a reload.

const server = () => new URL(process.env.SWARM_SERVE_URL!);

async function ready(tab: Page) {
  await tab.waitForFunction(() => window.__swarmReady === true);
}

const workspaceRow = (tab: Page, name: string) => tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: name });

async function openChat(tab: Page, session: string) {
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.locator(`[data-testid="tab"][data-session="${session}"]`)).toHaveAttribute("aria-selected", "true");
}

test("the URL follows the open chat, a reload keeps the token and the chat, and back steps", async ({ page: tab }) => {
  const errors: string[] = [];
  // WebKit reports a fetch that a reload or goto cancels as "... due to access control checks".
  const expected = (m: string) => m.startsWith("ResizeObserver loop") || m.endsWith("due to access control checks.");
  tab.on("pageerror", (e) => !expected(e.message) && errors.push(e.message));
  await tab.goto(`${server().origin}/${server().hash}`);
  await ready(tab);
  // The token left the URL.
  expect(new URL(tab.url()).hash).toBe("");
  const chats = workspaceRow(tab, "ws-tabs").locator("xpath=following-sibling::*[@data-kind='chat']");
  const [a, b] = await Promise.all([0, 1].map(async (i) => (await chats.nth(i).getAttribute("data-session"))!));
  await openChat(tab, a);
  await expect(tab).toHaveURL(`${server().origin}/chat/${a}`);
  await openChat(tab, b);
  await expect(tab).toHaveURL(`${server().origin}/chat/${b}`);
  await expect(tab.getByRole("button", { name: `Copy chat id ${b}` })).toHaveText(/^[0-9a-f-]{8,}$/);
  const short = (await tab.getByRole("button", { name: `Copy chat id ${b}` }).textContent())!;
  expect(b.startsWith(short)).toBe(true);

  // No hash now: the saved token opens the page, on the chat that the URL names.
  await tab.reload();
  await ready(tab);
  await expect(tab.locator(`[data-testid="tab"][data-session="${b}"]`)).toHaveAttribute("aria-selected", "true");

  await tab.goBack();
  await expect(tab).toHaveURL(`${server().origin}/chat/${a}`);
  await expect(tab.locator(`[data-testid="tab"][data-session="${a}"]`)).toHaveAttribute("aria-selected", "true");

  // A short id in a typed URL opens its chat.
  await tab.goto(`${server().origin}/chat/${short}`);
  await ready(tab);
  await expect(tab.locator(`[data-testid="tab"][data-session="${b}"]`)).toHaveAttribute("aria-selected", "true");
  await expect(tab).toHaveURL(`${server().origin}/chat/${b}`);
  expect(errors).toEqual([]);
});

test("a token that the server refuses is forgotten", async ({ page: tab }) => {
  await tab.goto(`${server().origin}/#token=${"0".repeat(64)}`);
  await expect(tab.getByText("Open the link that swarm ui printed.")).toBeVisible();
  expect(await tab.evaluate(() => localStorage.getItem("swarm.token"))).toBeNull();
});
