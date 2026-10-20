import { expect, type Page } from "@playwright/test";

/** Opens each chat until one shows a tool card with this title. */
async function openChatWithCard(tab: Page, title: string) {
  const chats = tab.getByTestId("sidebar").locator('[data-testid="tree-row"][data-kind="chat"]');
  for (let i = 0; i < (await chats.count()); i++) {
    await chats.nth(i).click();
    const chatId = (await chats.nth(i).getAttribute("data-session"))!;
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${chatId}"][data-state="ready"]`).getByTestId("row").first()).toBeVisible();
    const card = tab.getByTestId("tool-card").filter({ hasText: title });
    if (await card.count()) return card.first();
  }
  throw new Error(`no chat has a "${title}" card`);
}

export { openChatWithCard };
