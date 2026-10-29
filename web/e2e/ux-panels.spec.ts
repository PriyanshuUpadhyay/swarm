import { expect, test } from "@playwright/test";
import { open } from "./ux-helpers";

test("terminal shortcut takes focus, retains the selected shell, and Find stays in the terminal", async ({ page: tab }, info) => {
  await open(tab);
  await tab.getByRole("button", { name: "Terminal and background tasks", exact: true }).click();
  const panel = tab.getByTestId("background-panel").last();
  await expect(panel.locator(".xterm-helper-textarea")).toBeFocused();
  await panel.getByTestId("new-shell").click();
  await expect(panel.locator('button[aria-current="true"]')).toContainText("Shell");
  await expect(panel.locator(".xterm-helper-textarea")).toBeFocused();
  await tab.keyboard.press("Meta+f");
  await expect(panel.getByRole("textbox", { name: "Find in terminal", exact: true })).toBeFocused();
  await expect(tab.getByTestId("find-bar")).toHaveCount(0);
  await tab.keyboard.press("Escape");
  await tab.keyboard.press("Meta+j");
  await tab.keyboard.press("Meta+j");
  await expect(panel.locator('button[aria-current="true"]')).toContainText("Shell");
  await tab.screenshot({ path: info.outputPath("terminal-focus.png") });
});

test("Changes keeps an editor across views, filters files, saves added comments and finds in the diff", async ({ page: tab }, info) => {
  await open(tab, process.env.SWARM_E2E_GIT!);
  await tab.getByTestId("show-changes").click();
  const filter = tab.getByRole("textbox", { name: "Filter changed files", exact: true });
  await filter.fill("README");
  await expect(tab.getByTestId("change-row")).toHaveCount(1);
  await expect(tab.getByTestId("change-row")).toHaveCount(1);
  await tab.getByTestId("change-row").click();
  await tab.getByTestId("diff-body").locator('[data-pick]').last().click();
  await tab.getByTestId("comment-input").fill("Keep this review text");
  await tab.getByTestId("show-file").click();
  await tab.getByRole("button", { name: "Diff", exact: true }).click();
  await expect(tab.getByTestId("comment-input")).toHaveValue("Keep this review text");
  await tab.getByTestId("comment-add").click();
  await tab.getByTestId("diff-body").focus();
  await tab.keyboard.press("Meta+f");
  await tab.getByRole("textbox", { name: "Find in diff", exact: true }).fill("repository");
  await expect(tab.getByRole("search", { name: "Find in diff", exact: true })).toContainText("of");
  await expect(tab.getByTestId("find-bar")).toHaveCount(0);
  await tab.reload();
  await expect(tab.getByTestId("workspace-panel")).toBeVisible();
  await expect(tab.getByTestId("drafts")).toContainText("Keep this review text");
  await expect(tab.getByTestId("diff-path")).toBeVisible();
  await tab.screenshot({ path: info.outputPath("changes-persistence.png") });
});

test("pane keys move the zoom target and grips resize with arrows and Home", async ({ page: tab }, info) => {
  await open(tab, process.env[`SWARM_E2E_PANES_${info.project.name}`]!);
  const reviewer = tab.locator('[data-pane="reviewer"]');
  const tester = tab.locator('[data-pane="tester"]');
  const grip = tab.getByRole("separator", { name: "Split of reviewer", exact: true });
  await grip.focus();
  const height = (await reviewer.boundingBox())!.height;
  await grip.press("ArrowDown");
  await expect.poll(async () => (await reviewer.boundingBox())!.height).toBeGreaterThan(height + 15);
  await expect.poll(async () => Number(await grip.getAttribute("aria-valuenow"))).toBeGreaterThan(height);
  await grip.press("Home");
  await expect.poll(async () => (await reviewer.boundingBox())!.height).toBeCloseTo(height, -1);
  await tab.keyboard.press("Alt+Meta+ArrowRight");
  await tab.keyboard.press("Shift+Meta+Enter");
  await expect(reviewer).toHaveAttribute("data-zoomed", "true");
  await expect(tester).toHaveAttribute("inert", "");
  await tab.keyboard.press("Alt+Meta+ArrowDown");
  await expect(tester).toHaveAttribute("data-zoomed", "true");
  await expect(reviewer).toHaveAttribute("inert", "");
  await expect(tester.getByTestId("composer")).toBeFocused();
});

test("a terminal ticket failure can reconnect to the same target with a fresh ticket", async ({ page: tab }) => {
  const targets: unknown[] = [];
  await tab.route("**/api/terminal/ticket", async (route) => {
    targets.push(route.request().postDataJSON());
    if (targets.length === 1) return route.fulfill({ status: 503, body: "Attach offline" });
    return route.continue();
  });
  await open(tab);
  await tab.keyboard.press("Meta+j");
  const panel = tab.getByTestId("background-panel").last();
  await expect(panel.getByRole("status")).toContainText("Attach offline");
  await panel.getByRole("button", { name: "Reconnect", exact: true }).click();
  await expect.poll(() => targets.length).toBe(2);
  expect(targets[1]).toEqual(targets[0]);
  await expect(panel.getByRole("button", { name: "Reconnect", exact: true })).toHaveCount(0);
});

test("a successful shell create with a failed list read retains its id and offers a read-only retry", async ({ page: tab }) => {
  let creates = 0;
  await tab.route("**/api/shells", async (route) => { creates++; return route.fulfill({ json: { id: "created-shell" } }); });
  let fail = false;
  await tab.route("**/api/shells?*", (route) => route.fulfill(fail ? { status: 503, body: "List offline" } : { json: { shells: creates ? [{ id: "created-shell", command: "zsh" }] : [] } }));
  await tab.route("**/api/terminal/ticket", (route) => route.fulfill({ status: 503, body: "No mock terminal" }));
  await open(tab);
  await tab.keyboard.press("Meta+j");
  const panel = tab.getByTestId("background-panel").last();
  await expect(panel.getByTestId("new-shell")).toBeVisible();
  fail = true;
  await panel.getByTestId("new-shell").click();
  await expect(panel.getByRole("alert")).toContainText("Shell created. Could not refresh");
  await expect(panel.locator('[aria-current="true"]')).toContainText("Shell");
  fail = false;
  await panel.getByRole("button", { name: "Refresh shells", exact: true }).click();
  await expect(panel.getByRole("alert")).toHaveCount(0);
  expect(creates).toBe(1);
});

test("a visible Changes panel refreshes on window focus and keeps the selected file and editor", async ({ page: tab }) => {
  let reads = 0;
  await tab.route("**/api/changes?*", async (route) => { reads++; return route.continue(); });
  await open(tab, process.env.SWARM_E2E_GIT!);
  await tab.getByTestId("show-changes").click();
  await tab.getByRole("textbox", { name: "Filter changed files", exact: true }).fill("README");
  await expect(tab.getByTestId("change-row")).toHaveCount(1);
  await tab.getByTestId("change-row").click();
  await tab.getByTestId("diff-body").locator('[data-pick]').last().click();
  await tab.getByTestId("comment-input").fill("Keep while refreshing");
  await expect(tab.getByTestId("comment-input")).toHaveValue("Keep while refreshing");
  const previous = reads;
  await tab.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect.poll(() => reads).toBeGreaterThan(previous);
  await expect(tab.getByTestId("diff-path")).toHaveText("README.md");
  await expect(tab.getByTestId("comment-input")).toHaveValue("Keep while refreshing");
});
