import { expect, test, type Locator, type Page } from "@playwright/test";

// The server side of a branch is tested in src/serve/branch/tests.rs; the e2e chats run no CLI
// that could resume a cut session, so the branch call answers with another listed chat.
test("Branch from here and Esc Esc start a chat from an earlier message", async ({ page: tab }) => {
  const live = process.env.SWARM_E2E_TYPED_CHAT!;
  const other = process.env.SWARM_E2E_GIT!;
  const asked: unknown[] = [];
  await tab.route("**/api/chat/branch", async (route) => {
    asked.push(route.request().postDataJSON());
    await route.fulfill({ json: { ok: true, session: other, agent: "orchestrator", pane: "%1", text: "the message again" } });
  });
  const composer = await openLive(tab);

  // Esc Esc in the empty box lists the chat's messages; Esc closes the list.
  const picker = await openPicker(tab, composer);
  await expect(picker.getByRole("option").first()).toBeVisible();
  await tab.keyboard.press("Escape");
  await expect(picker).toHaveCount(0);
  await expect(composer).toBeFocused();
  await tab.keyboard.type("typed after");
  await expect(composer).toHaveValue("typed after");

  const bubble = tab.locator(".bubble-row").first();
  await bubble.hover();
  await bubble.getByTestId("branch-here").click();
  await expect(tab.getByTestId("confirm-body")).toContainText("Branches the conversation");
  await tab.getByTestId("confirm-action").click();
  await expect(tab.locator(`[data-testid="tree-row"][data-session="${other}"]`)).toHaveAttribute("aria-current", /./);
  await expect(composer).toHaveValue("the message again");
  expect(asked).toEqual([{ session: live, row: expect.stringMatching(/@raw-\d+-\d+$/) }]);
  await composer.fill("");
});

async function openLive(tab: Page) {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_TYPED_CHAT!}"]`).click();
  const composer = tab.getByTestId("composer");
  await expect(composer).toBeEnabled();
  return composer;
}

/** Esc Esc in the empty box opens the picker of the chat's messages. */
async function openPicker(tab: Page, composer: Locator) {
  await composer.press("Escape");
  await composer.press("Escape");
  return tab.getByTestId("branch-picker");
}

