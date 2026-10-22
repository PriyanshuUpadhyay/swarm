import { expect, test } from "@playwright/test";

// The server side of a branch is tested in src/serve/branch/tests.rs; the e2e chats run no CLI
// that could resume a cut session, so the branch call answers with another listed chat.
test("Branch from here and Esc Esc start a chat from an earlier message", async ({ page: tab }) => {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  const live = process.env.SWARM_E2E_TYPED!;
  const other = process.env.SWARM_E2E_GIT!;
  const asked: unknown[] = [];
  await tab.route("**/api/chat/branch", async (route) => {
    asked.push(route.request().postDataJSON());
    await route.fulfill({ json: { ok: true, session: other, agent: "orchestrator", pane: "%1", text: "the message again" } });
  });
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${live}"]`).click();
  const composer = tab.getByTestId("composer");
  await expect(composer).toBeEnabled();

  // Esc Esc in the empty box lists the chat's messages; Esc closes the list.
  await composer.press("Escape");
  await composer.press("Escape");
  const picker = tab.getByTestId("branch-picker");
  await expect(picker.getByRole("option").first()).toBeVisible();
  await tab.keyboard.press("Escape");
  await expect(picker).toHaveCount(0);

  const bubble = tab.locator(".bubble-row").first();
  await bubble.hover();
  await bubble.getByTestId("branch-here").click();
  await expect(tab.getByTestId("confirm-body")).toContainText("Branches the conversation, not the files.");
  await tab.getByTestId("confirm-action").click();
  await expect(tab.locator(`[data-testid="tree-row"][data-session="${other}"]`)).toHaveAttribute("aria-current", /./);
  await expect(composer).toHaveValue("the message again");
  expect(asked).toEqual([{ session: live, row: expect.stringMatching(/@raw-\d+-\d+$/) }]);
  await composer.fill("");
});

async function openLive(tab: import("@playwright/test").Page) {
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env.SWARM_E2E_TYPED!}"]`).click();
  const composer = tab.getByTestId("composer");
  await expect(composer).toBeEnabled();
  return composer;
}

test("a picker opened before the rows load lists them when they arrive", async ({ page: tab }) => {
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await tab.route("**/api/rows?**", async (route) => {
    await held;
    await route.continue();
  });
  const composer = await openLive(tab);
  await composer.press("Escape");
  await composer.press("Escape");
  const picker = tab.getByTestId("branch-picker");
  await expect(picker).toBeVisible();
  await expect(picker.getByRole("option")).toHaveCount(0);
  release();
  await expect(picker.getByRole("option").first()).toBeVisible();
});

test("a held Esc does not open the picker", async ({ page: tab }) => {
  const composer = await openLive(tab);
  await composer.focus();
  // The second keydown of a held key is an auto-repeat.
  await tab.keyboard.down("Escape");
  await tab.keyboard.down("Escape");
  await tab.keyboard.up("Escape");
  await expect(tab.getByTestId("branch-picker")).toHaveCount(0);
});

test("Esc closes the picker and the keys go back to the text box", async ({ page: tab }) => {
  const composer = await openLive(tab);
  await composer.press("Escape");
  await composer.press("Escape");
  await expect(tab.getByTestId("branch-picker")).toBeVisible();
  await tab.keyboard.press("Escape");
  await expect(tab.getByTestId("branch-picker")).toHaveCount(0);
  await expect(composer).toBeFocused();
  await tab.keyboard.type("typed after");
  await expect(composer).toHaveValue("typed after");
});

test("the picker says it is loading, shows a failed load with Retry, and says older messages are not listed", async ({ page: tab }) => {
  let mode: "hold" | "fail" | "older" = "hold";
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await tab.route("**/api/rows?**", async (route) => {
    if (mode === "hold") await held;
    // The mode is read again after the wait, so the held request can fail.
    if (mode === "hold") {
      await route.continue();
    } else if (mode === "fail") {
      await route.fulfill({ status: 500, json: { error: "rows are gone" } });
    } else {
      const page = await (await route.fetch()).json();
      page.page.start_offset = 5;
      await route.fulfill({ json: page });
    }
  });
  const composer = await openLive(tab);
  await composer.press("Escape");
  await composer.press("Escape");
  const picker = tab.getByTestId("branch-picker");
  await expect(picker.getByText("Loading messages…")).toBeVisible();
  await expect(picker.getByText("No messages")).toHaveCount(0);
  mode = "fail";
  release();
  await expect(picker.getByRole("alert")).toContainText("rows are gone");
  mode = "older";
  await picker.getByRole("button", { name: "Retry" }).click();
  await expect(picker.getByRole("option").first()).toBeVisible();
  await expect(picker.getByText("Older messages are not listed. Scroll up to branch from one.")).toBeVisible();
});
