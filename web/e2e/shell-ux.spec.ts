import { expect, test, type Page } from "@playwright/test";
import { resolve } from "node:path";
import { header, open } from "./lifecycle-helpers";

async function settings(page: Page, section = "profiles") {
  await page.getByTestId("settings-button").click();
  await page.getByTestId(`settings-page-${section}`).click();
  return page.getByTestId("settings");
}
async function screenshot(page: Page, name: string) {
  await page.screenshot({ path: resolve(import.meta.dirname, `../../tmp/shell-ux/${name}.png`) });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
}

test("Settings is visible, invalid usage has a range, runner drafts survive Escape and page switches, and models are suggested", async ({ page }) => {
  await page.route("**/api/models?*", (route) => route.fulfill({ json: { models: [{ id: "suggested-model", label: "Suggested model" }] } }));
  await open(page);
  const dialog = await settings(page);
  const usage = dialog.getByTestId("min-usage");
  await usage.fill("150");
  await usage.press("Enter");
  await expect(usage).toHaveAttribute("aria-invalid", "true");
  await expect(dialog).toContainText("Enter 0 to 100.");
  const row = dialog.locator('[data-testid="profile-row"][data-name="chat"]');
  await row.locator('[data-action="edit"]').click();
  await expect(usage).toHaveValue("150");
  await expect(usage).toHaveAttribute("aria-invalid", "true");
  const model = row.getByTestId("runner-model").first();
  await model.fill("unsaved-custom-model");
  await expect(row.locator("datalist option").first()).toHaveAttribute("value", "suggested-model");
  await model.press("Escape");
  await expect(dialog).toBeVisible();
  await row.locator('[data-action="edit"]').click();
  await expect(model).toHaveValue("unsaved-custom-model");
  await dialog.getByTestId("settings-page-appearance").click();
  await dialog.getByRole("button", { name: "Close Settings" }).click();
  await settings(page);
  await row.locator('[data-action="edit"]').click();
  await expect(model).toHaveValue("unsaved-custom-model");
  await screenshot(page, "profile-draft");
});

test("a scrolled Settings confirmation stays visible and focuses Cancel", async ({ page }) => {
  await open(page);
  const dialog = await settings(page);
  const rows = dialog.getByTestId("profile-row");
  const last = rows.last();
  await last.locator('[data-action="delete"]').click();
  const confirm = dialog.getByTestId("settings-confirm");
  await expect(confirm).toBeInViewport();
  await expect(confirm.getByRole("button", { name: "Cancel" })).toBeFocused();
  await screenshot(page, "confirmation");
});

test("signed-out accounts have a login action and the native default has a CLI step", async ({ page }) => {
  await page.route("**/api/accounts?*", (route) => route.fulfill({ json: {
    provider: "claude", state: "ready", revision: "test", auto: null,
    accounts: [{ name: "work", auth_state: "signed_out", email: null }, { name: "default", auth_state: "signed_out", email: null }],
  } }));
  let login: unknown;
  await page.route("**/api/accounts/login", (route) => { login = route.request().postDataJSON(); return route.fulfill({ json: { pane: "test-pane" } }); });
  await open(page);
  const dialog = await settings(page, "accounts");
  const claude = dialog.getByTestId("accounts-claude");
  await expect(claude.locator('[data-name="default"]')).toContainText("claude /login");
  await claude.locator('[data-name="work"]').getByRole("button", { name: "Sign in…" }).click();
  await expect.poll(() => login).toMatchObject({ provider: "claude", name: "work" });
  await expect(dialog.getByTestId("login-note")).toBeInViewport();
  await screenshot(page, "accounts");
});

test("setup script keeps load errors separate, focuses after Retry, and keeps a refused draft", async ({ page }) => {
  let fail = true;
  await page.route("**/api/workspace/new-options?*", (route) => fail
    ? route.fulfill({ status: 503, json: { error: "temporary read failure" } })
    : route.fulfill({ json: { setup_script: "saved script" } }));
  await page.route("**/api/choices", (route) => route.request().postDataJSON().op === "setup_script"
    ? route.fulfill({ status: 400, json: { error: "refused write" } }) : route.continue());
  await open(page);
  await header(page, "proj-gh").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Setup script…" }).click();
  const dialog = page.getByTestId("setup-script-dialog");
  await expect(dialog.getByTestId("setup-script-save")).toBeDisabled();
  await expect(dialog.getByTestId("dialog-error")).toContainText("temporary read failure");
  fail = false;
  await dialog.getByRole("button", { name: "Retry" }).click();
  const box = dialog.getByTestId("setup-script");
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("saved script");
  await box.fill("retained draft");
  await dialog.getByTestId("setup-script-save").click();
  await expect(dialog.getByTestId("dialog-error")).toContainText("Your draft is kept");
  await expect(box).toHaveValue("retained draft");
  await screenshot(page, "setup-script");
});

test("chat pickers filter, move with arrow keys, and refresh imports without closing", async ({ page }) => {
  await page.route("**/api/sessions/archived", (route) => route.fulfill({ json: { sessions: [
    { id: "a", title: "Build tests", cwd: "/repo", chair_provider: "claude", archivedAt: 42 },
    { id: "b", title: "Fix theme", cwd: "/repo", chair_provider: "codex", archivedAt: 41 },
  ] } }));
  let imports = 0;
  await page.route("**/api/chats/importable", (route) => route.fulfill({ json: { chats: [
    { id: "one", title: `Imported choice ${++imports}`, workspace: "/repo", provider: "codex", lastActivity: 42 },
  ] } }));
  await open(page);
  await page.getByRole("button", { name: "Add project" }).click();
  await page.getByRole("menuitem", { name: "Archived chats…" }).click();
  const archive = page.getByTestId("recently-closed");
  const filter = archive.getByRole("searchbox");
  await expect(filter).toBeFocused();
  await filter.fill("theme");
  await expect(archive.getByTestId("closed-chat")).toHaveCount(1);
  await filter.press("ArrowDown");
  await expect(archive.getByTestId("closed-chat")).toBeFocused();
  await screenshot(page, "archived-search");
  await archive.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: "Add project" }).click();
  await page.getByRole("menuitem", { name: "Import chats…" }).click();
  const dialog = page.getByTestId("import-chats");
  await expect(dialog.getByTestId("importable-chat")).toContainText("Imported choice 1");
  await dialog.getByRole("button", { name: "Refresh" }).click();
  await expect(dialog.getByTestId("importable-chat")).toContainText("Imported choice 2");
  await dialog.getByRole("searchbox").fill("not present");
  await expect(dialog).toContainText("No chat matches.");
});

test("a new workspace clears a selected issue when the project changes and uses Hide while working", async ({ page }) => {
  await page.route("**/api/workspace/new", () => {});
  await open(page);
  await header(page, "proj-gh").hover();
  await header(page, "proj-gh").locator('[data-action="new-workspace"]').click();
  const dialog = page.getByTestId("new-workspace");
  await dialog.getByTestId("nw-mode-issue").click();
  await dialog.getByTestId("nw-issue").first().click();
  await expect(dialog.getByTestId("nw-create")).toBeEnabled();
  const project = dialog.getByTestId("nw-project");
  const other = await project.locator("option").evaluateAll((options, selected) => options.map((o) => (o as HTMLOptionElement).value).find((value) => value !== selected), await project.inputValue());
  expect(other).toBeTruthy();
  await project.selectOption(other!);
  await expect(dialog.getByTestId("nw-create")).toBeDisabled();
  await dialog.getByTestId("nw-mode-new-branch").click();
  await dialog.getByTestId("nw-name").fill("pending-setup");
  await expect(dialog.getByTestId("nw-folder")).not.toHaveValue("");
  await dialog.getByTestId("nw-create").click();
  await expect(dialog.getByRole("button", { name: "Hide", exact: true })).toBeVisible();
  await expect(dialog).toContainText("Setup continues when you hide this dialog.");
  await screenshot(page, "workspace-working");
});

test("Settings shows notice health after Home dismissal and Home includes setup", async ({ page }) => {
  await page.route("**/api/notices", (route) => route.fulfill({ json: { state: "held", reason: "Focus holds notices", steps: [], action: "focus" } }));
  await open(page);
  await page.getByTestId("home-button").click();
  const card = page.getByTestId("notices-card");
  await expect(card).toContainText("Focus holds notices");
  await card.getByRole("button", { name: "Not now" }).click();
  await page.reload();
  await page.waitForFunction(() => window.__swarmReady === true);
  await expect(page.getByTestId("notices-card")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Set up this Mac" })).toBeVisible();
  const dialog = await settings(page, "notifications");
  await expect(dialog.getByRole("region", { name: "System notification status" })).toContainText("Focus holds notices");
  await expect(dialog.getByRole("button", { name: "Send a test notice" })).toBeVisible();
  await screenshot(page, "notice-settings");
});

test("the Skills catalog focuses and keeps its filter across a view switch", async ({ page }) => {
  await open(page);
  await page.getByTestId("skills-button").click();
  const filter = page.getByTestId("skills-filter");
  await expect(filter).toBeFocused();
  await filter.fill("review");
  await page.getByTestId("skills-view-graph").click();
  await page.getByTestId("skills-view-catalog").click();
  await expect(filter).toHaveValue("review");
  await screenshot(page, "skills-filter");
});

test("Later leaves the installed update available in Settings", async ({ page }) => {
  await page.route("**/api/update", (route) => route.request().method() === "GET"
    ? route.fulfill({ json: { available: true, latest: "9.9.9" } })
    : route.fulfill({ json: { version: "9.9.9", hud: false } }));
  await open(page);
  await page.getByRole("button", { name: "Upgrade", exact: true }).click();
  await page.getByRole("button", { name: "Later", exact: true }).click();
  const dialog = await settings(page, "appearance");
  await expect(dialog).toContainText("Swarm 9.9.9 is installed");
  await dialog.getByRole("button", { name: "Restart server…" }).click();
  await expect(page.getByRole("button", { name: "Restart server", exact: true })).toBeVisible();
  await screenshot(page, "pending-update");
});
