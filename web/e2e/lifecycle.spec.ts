import { expect, test } from "@playwright/test";
import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { api, header, open, readTree, server, shoot, workspace } from "./lifecycle-helpers";

// New workspace in each mode, setup script output, switch model cancel, archive and delete, and
// Recently closed (S3), plus the four V1 fixes. Both browsers share one server home, so each case
// names its workspaces after the browser and puts back any choice it changes.

test("Archive chat moves a chat to Archived, and ⇧⌘T with no closed tab opens the archived list", async ({ page: tab }) => {
  const errors = await open(tab);
  const session = (await workspace(tab, "ws-edit").locator("xpath=following-sibling::*[@data-kind='chat']").first().getAttribute("data-session"))!;
  const row = tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
  try {
    await row.click({ button: "right" });
    await tab.getByRole("menuitem", { name: "Archive chat" }).click();
    await expect(row).toBeHidden();

    await tab.getByTestId("sidebar").focus();
    await tab.keyboard.press("Shift+Meta+t");
    const closed = tab.getByTestId("recently-closed");
    await expect(closed).toBeVisible();
    await closed.locator(`[data-testid="closed-chat"][data-session="${session}"]`).click();
    await expect(closed).toBeHidden();
    await expect(row).toBeVisible();
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`)).toBeVisible();
  } finally {
    const { sessions } = await api<{ sessions: { id: string }[] }>("/api/sessions/archived");
    if (sessions.some((s) => s.id === session)) await api("/api/sessions/unarchive", { ids: [session] });
  }
  expect(errors).toEqual([]);
});

test("⌘-click picks two chats, and the row menu archives both at once", async ({ page: tab }) => {
  const errors = await open(tab);
  // Two chats with ended chairs, so no other case loses a live agent.
  const chatOf = (ws: string) => workspace(tab, ws).locator("xpath=following-sibling::*[@data-kind='chat']").first().getAttribute("data-session");
  const sessions = [(await chatOf("ws-build"))!, (await chatOf("ws-edit"))!];
  const rows = sessions.map((s) => tab.locator(`[data-testid="tree-row"][data-session="${s}"]`));
  try {
    await rows[0].click({ modifiers: ["Meta"] });
    await rows[1].click({ modifiers: ["Meta"] });
    for (const row of rows) await expect(row).toHaveAttribute("data-marked", "true");
    await rows[1].click({ button: "right" });
    await expect(tab.getByTestId("row-menu")).toContainText("2 chats selected");
    await tab.getByRole("menuitem", { name: "Archive 2 chats" }).click();
    const confirm = tab.getByTestId("confirm-action");
    if (await confirm.isVisible()) await confirm.click();
    for (const row of rows) await expect(row).toBeHidden();
  } finally {
    const { sessions: archived } = await api<{ sessions: { id: string }[] }>("/api/sessions/archived");
    const back = sessions.filter((s) => archived.some((a) => a.id === s));
    if (back.length) await api("/api/sessions/unarchive", { ids: back });
  }
  expect(errors).toEqual([]);
});

test("an archived chat folds under its workspace, and Unarchive opens it again", async ({ page: tab }) => {
  const errors = await open(tab);
  const session = (await workspace(tab, "ws-edit").locator("xpath=following-sibling::*[@data-kind='chat']").first().getAttribute("data-session"))!;
  const row = tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
  const archived = tab.locator(`[data-testid="tree-row"][data-id="archived-chat:${session}"]`);
  try {
    await row.click({ button: "right" });
    await tab.getByRole("menuitem", { name: "Archive chat" }).click();
    await expect(row).toBeHidden();
    const fold = workspace(tab, "ws-edit").locator("xpath=following-sibling::*[@data-kind='archived']").first();
    await expect(fold).toHaveText(/Archived · \d+/);
    // The other browser's run can leave the fold open in the shared choices.
    if ((await fold.getAttribute("aria-expanded")) === "true") await fold.click();
    await expect(fold).toHaveAttribute("aria-expanded", "false");
    await expect(archived).toHaveCount(0);
    await fold.click();
    await expect(archived).toBeVisible();
    // A click on the row only selects it; the button unarchives.
    await archived.click();
    await expect(archived).toBeVisible();
    await archived.click({ button: "right" });
    await expect(tab.getByRole("menuitem", { name: "Unarchive chat" })).toBeVisible();
    await tab.keyboard.press("Escape");
    // After a long run, headless WebKit hit-tests this scrolled sidebar at stale positions, so a
    // pointer click lands on the tree; the click goes to the button itself.
    await archived.hover();
    await archived.locator('[data-action="unarchive"]').dispatchEvent("click");
    await expect(row).toBeVisible();
    await expect(archived).toHaveCount(0);
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`)).toBeVisible();
  } finally {
    const { sessions } = await api<{ sessions: { id: string }[] }>("/api/sessions/archived");
    if (sessions.some((s) => s.id === session)) await api("/api/sessions/unarchive", { ids: [session] });
  }
  expect(errors).toEqual([]);
});

test("A chat whose chair has no pane shows only its log, and Continue chat opens Switch model", async ({ page: tab }) => {
  const errors = await open(tab);
  const session = (await workspace(tab, "ws-edit").locator("xpath=following-sibling::*[@data-kind='chat']").first().getAttribute("data-session"))!;
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`)).toBeVisible();
  await expect(tab.getByTestId("composer")).toHaveCount(0);
  await expect(tab.getByTestId("composer-status")).toHaveCount(0);
  await tab.getByTestId("continue-chat").click();
  await expect(tab.getByTestId("switch-model")).toBeVisible();
  await tab.keyboard.press("Escape");
  expect(errors).toEqual([]);
});

test("The run chip hands off to another CLI with a summary, and Cancel stops it while the chair writes it", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  const chats = async () => (await readTree()).sections.flatMap((s) => s.rows).filter((r) => r.kind === "chat").length;
  const before = await chats();
  await tab.locator(`[data-testid="tree-row"][data-session="${live}"]`).click();
  await tab.getByTestId("model-chip").click();
  const dialog = tab.getByTestId("run-popover");
  await expect(dialog).toBeVisible();
  await dialog.getByTestId("run-query").fill("gpt-e2e");
  // The same CLI restarts its own session and asks nothing.
  await dialog.getByTestId("run-custom-claude").click();
  await expect(dialog.getByTestId("run-outcome")).toHaveText("Claude restarts on the new settings. The chat keeps its history.");
  await expect(dialog.getByTestId("run-carry")).toHaveCount(0);
  await dialog.getByTestId("run-custom-codex").click();
  // Another CLI asks what to bring, with Summary as the default.
  await expect(dialog.getByTestId("run-carry-choice").locator('[aria-checked="true"]')).toHaveText("Summary");
  await expect(dialog.getByTestId("run-outcome")).toHaveText("A new Codex chat starts and gets a summary of this one.");
  await shoot(tab, "run-popover", info.project.name);
  await dialog.getByTestId("run-apply").click();
  const progress = dialog.getByTestId("sm-progress");
  // The live chair was asked for a summary and does not answer, so the switch waits here.
  await expect(progress).toHaveAttribute("data-phase", "summarizing");
  await shoot(tab, "switch-model", info.project.name);
  await dialog.getByTestId("run-cancel").click();
  await expect(progress).toHaveAttribute("data-phase", "cancelled");
  await expect(progress).toContainText("Cancelled. This chat is unchanged.");
  expect(await chats()).toBe(before);
  await tab.keyboard.press("Escape");
  await expect(tab.locator(`[data-testid="transcript"][data-chat="${live}"]`)).toBeVisible();
  expect(errors).toEqual([]);
});

test("Create Project on an existing folder offers Import, Import asks about a plain folder, and Remove Project hides it again", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const folder = join(process.env.SWARM_E2E_DIR!, `plain-${info.project.name}`);
  mkdirSync(folder, { recursive: true });
  await tab.getByRole("button", { name: "Add project" }).click();
  await tab.getByRole("menuitem", { name: "Create Project…" }).click();
  const dialog = tab.getByTestId("project-dialog");
  await dialog.getByTestId("project-path").fill(folder);
  await dialog.getByTestId("project-save").click();
  await expect(dialog.getByTestId("project-exists")).toContainText("This folder already exists");
  await shoot(tab, "create-exists", info.project.name);
  await dialog.getByRole("button", { name: "Import this folder" }).click();
  await expect(dialog).toContainText("Import project");
  await expect(dialog.getByTestId("project-path")).toHaveValue(folder);
  await dialog.getByTestId("project-save").click();
  await expect(dialog.getByTestId("import-choice")).toContainText("not a Git repository");
  await dialog.getByRole("button", { name: "Use as a plain folder" }).click();
  await expect(dialog).toBeHidden();
  const project = header(tab, `plain-${info.project.name}`);
  await expect(project).toBeVisible();

  await project.click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Remove Project…" }).click();
  await expect(tab.getByTestId("confirm-body")).toContainText(`Hide plain-${info.project.name} from the sidebar?`);
  await tab.getByTestId("confirm-action").click();
  await expect(project).toBeHidden();
  expect(existsSync(folder)).toBe(true);
  expect(errors).toEqual([]);
});

// ---- V1 fixes ----

test("Shift+F10 and the ContextMenu key open the focused row's menu", async ({ page: tab }) => {
  const errors = await open(tab);
  const row = workspace(tab, "ws-build");
  await row.click();
  for (const key of ["Shift+F10", "ContextMenu"]) {
    await row.focus();
    await tab.keyboard.press(key);
    const menu = tab.getByTestId("row-menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Rename workspace…" })).toBeVisible();
    await tab.keyboard.press("Escape");
    await expect(menu).toBeHidden();
  }
  expect(errors).toEqual([]);
});

test("holding ⌘ over a row shows its actions as icons, and releasing ⌘ hides them", async ({ page: tab }) => {
  const errors = await open(tab);
  const row = workspace(tab, "ws-build");
  const bar = tab.getByTestId("row-bar");
  await expect(tab.getByTestId("row-bar-hint")).toBeVisible();
  // The new chat "+" shows without hover.
  await tab.mouse.move(0, 0);
  await expect(row.locator('[data-action="new-chat"]')).toBeVisible();
  await row.hover();
  await expect(bar).toBeHidden();
  await tab.keyboard.down("Meta");
  await expect(bar).toBeVisible();
  await expect(bar.getByRole("button", { name: "Rename workspace…" })).toBeVisible();
  await bar.getByRole("button", { name: "Copy path" }).hover();
  await expect(bar).toBeVisible();
  await tab.keyboard.up("Meta");
  await expect(bar).toBeHidden();

  // The Recent list has the same chat actions, by ⌘ hover and by right click.
  await tab.getByTestId("view-recent").click();
  const recent = tab.getByTestId("recent-row").first();
  await recent.hover();
  await tab.keyboard.down("Meta");
  await expect(bar.getByRole("button", { name: "Archive chat" })).toBeVisible();
  await tab.keyboard.up("Meta");
  await expect(bar).toBeHidden();
  await recent.click({ button: "right" });
  await expect(tab.getByTestId("row-menu").getByRole("menuitem", { name: "Rename chat…" })).toBeVisible();
  await tab.keyboard.press("Escape");
  await tab.getByTestId("view-workspaces").click();
  expect(errors).toEqual([]);
});

test("Tab reaches an agent pane's header buttons", async ({ page: tab }, info) => {
  const errors = await open(tab);
  await tab.setViewportSize({ width: 1600, height: 900 });
  await tab.locator(`[data-testid="tree-row"][data-session="${process.env[`SWARM_E2E_PANES_${info.project.name}`]}"]`).click();
  await expect(tab.getByTestId("pane")).toHaveCount(2);
  await tab.keyboard.press("Alt+Meta+ArrowRight");
  const inHeader = () => tab.evaluate(() => document.activeElement?.closest(".pane-head") != null && document.activeElement?.tagName === "BUTTON");
  let reached = false;
  // WebKit moves Tab to buttons only with Option held, as Safari does by default.
  const key = info.project.name === "webkit" ? "Alt+Tab" : "Tab";
  for (let i = 0; i < 40 && !reached; i++) {
    await tab.keyboard.press(key);
    reached = await inHeader();
  }
  expect(reached).toBe(true);
  expect(errors).toEqual([]);
});

test("the Search label meets 4.5:1 contrast in both themes", async ({ page: tab }) => {
  const errors = await open(tab);
  for (const theme of ["light", "dark"]) {
    const ratio = await tab.evaluate((t) => {
      document.documentElement.dataset.theme = t;
      const rgb = (c: string) => c.match(/[\d.]+/g)!.slice(0, 3).map(Number);
      const lum = ([r, g, b]: number[]) => {
        const f = (v: number) => ((v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
        return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
      };
      const label = document.querySelector(".search-box span")!;
      const fg = lum(rgb(getComputedStyle(label).color));
      const bg = lum(rgb(getComputedStyle(document.querySelector(".bg-sidebar")!).backgroundColor));
      return (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05);
    }, theme);
    expect(ratio, theme).toBeGreaterThanOrEqual(4.5);
  }
  expect(errors).toEqual([]);
});

test("Home's recent rows name the project beside the workspace", async ({ page: tab }) => {
  const errors = await open(tab);
  const row = workspace(tab, "ws-build");
  const path = (await row.getAttribute("data-id"))!;
  const rename = async (name: string) => {
    const { revision } = await readTree();
    await api("/api/choices", { revision, op: "rename_workspace", workspace: path, name });
  };
  // A renamed workspace differs from its project's name, so both show.
  await rename("main");
  try {
    await tab.goto(`${server().origin}/${server().hash}`);
    await tab.waitForFunction(() => window.__swarmReady === true);
    await tab.getByTestId("home-button").click();
    await expect(tab.getByTestId("recent-chat").filter({ hasText: "ws-build · main" })).toHaveCount(1);
  } finally {
    await rename("");
  }
  expect(errors).toEqual([]);
});

test("Import chats lists a chat that another adapter started, and importing it lists it in the sidebar", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const session = process.env[`SWARM_E2E_IMPORT_${info.project.name}`]!;
  const row = tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
  await expect(workspace(tab, `ws-import-${info.project.name}`)).toHaveCount(0);
  try {
    await tab.getByRole("button", { name: "Add project" }).click();
    await tab.getByRole("menuitem", { name: "Import chats…" }).click();
    const dialog = tab.getByTestId("import-chats");
    await dialog.locator(`[data-testid="importable-chat"][data-session="${session}"]`).click();
    await expect(dialog).toBeHidden();
    await expect(row).toBeVisible();
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`)).toBeVisible();
  } finally {
    // The other cases count the listed chats, so this one leaves the sidebar as it found it. The
    // server's refresh then drops the chat's tab and writes a new revision, so wait for it, else
    // the next case can load the old revision and get a 409.
    await api("/api/sessions/archive", { ids: [session] });
    await expect.poll(async () => JSON.stringify(await readTree()).includes(session), { timeout: 10_000 }).toBe(false);
  }
  expect(errors).toEqual([]);
});
