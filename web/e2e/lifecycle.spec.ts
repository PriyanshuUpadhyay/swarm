import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { ISSUES } from "./serve";

// New workspace in each mode, setup script output, switch model cancel, archive and delete, and
// Recently closed (S3), plus the four V1 fixes. Both browsers share one server home, so each case
// names its workspaces after the browser and puts back any choice it changes.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;
const SHOTS = resolve(import.meta.dirname, "../../tmp/web-port");

async function api<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${server().origin}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { Authorization: `Bearer ${token()}`, "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`);
  return (await res.json()) as T;
}

type Tree = { revision: string; sections: { kind: string; path: string | null; title: string; rows: { id: string; kind: string; title: string; selection: { chat: string | null } }[] }[] };
const readTree = (view = "workspaces") => api<Tree>(`/api/tree?view=${view}`);

async function open(tab: Page) {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  // A refused request logs its response as a console error; the cases that expect one check the UI.
  tab.on("console", (m) => m.type() === "error" && !/\b(400|404|409)\b/.test(m.text()) && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

const header = (tab: Page, project: string) => tab.locator('[data-testid="tree-row"][data-kind="project"]').filter({ hasText: project });
const workspace = (tab: Page, title: string) => tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: new RegExp(`^${title}`) });

/** Screenshots for the result, in both themes, only when E2E_SHOTS is set and in Chromium. */
async function shoot(tab: Page, name: string, browser: string) {
  if (!process.env.E2E_SHOTS || browser !== "chromium") return;
  for (const theme of ["dark", "light"])
    await tab.evaluate((t) => (document.documentElement.dataset.theme = t), theme).then(() => tab.screenshot({ path: join(SHOTS, `s3-${name}-${theme}.png`) }));
  await tab.evaluate(() => delete document.documentElement.dataset.theme);
}

/** Opens New Workspace from the project's "+", fills a name, and creates it. */
async function newWorkspace(tab: Page, project: string, fill: () => Promise<void>) {
  await header(tab, project).hover();
  await header(tab, project).locator('[data-action="new-workspace"]').click();
  const dialog = tab.getByTestId("new-workspace");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId("nw-folder")).not.toHaveValue("");
  await fill();
  await dialog.getByTestId("nw-create").click();
  return dialog;
}

test("New workspace makes a new branch, an existing branch and a pull request workspace", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const b = info.project.name;
  const dialog = tab.getByTestId("new-workspace");

  await newWorkspace(tab, "proj-gh", async () => {
    await dialog.getByTestId("nw-name").fill(`nb-${b}`);
    await expect(dialog.getByTestId("nw-base")).toHaveValue("main");
    await shoot(tab, "new-workspace", b);
  });
  await expect(dialog).toBeHidden();
  await expect(workspace(tab, `nb-${b}`)).toBeVisible();
  await expect(tab.getByTestId("workspace-landing")).toContainText(`nb-${b}`);

  // An existing branch fills the name from the branch.
  await newWorkspace(tab, "proj-gh", async () => {
    await dialog.getByTestId("nw-mode-existing-branch").click();
    await dialog.getByTestId("nw-branch").selectOption(`topic-${b}`);
    await expect(dialog.getByTestId("nw-name")).toHaveValue(`topic-${b}`);
  });
  await expect(dialog).toBeHidden();
  await expect(workspace(tab, `topic-${b}`)).toBeVisible();

  // A pull request is fetched from origin into a new branch.
  await newWorkspace(tab, "proj-pr", async () => {
    await dialog.getByTestId("nw-mode-pr").click();
    await dialog.getByTestId("nw-name").fill(`pr-${b}`);
    await dialog.getByTestId("nw-pr").fill("7");
  });
  await expect(dialog).toBeHidden();
  const pr = (await workspace(tab, `pr-${b}`).getAttribute("data-id"))!;
  expect(readFileSync(join(pr, "review.md"), "utf8")).toBe("the change under review\n");

  // A refusal stays in the dialog: the same name again has a folder already.
  await newWorkspace(tab, "proj-gh", async () => {
    await dialog.getByTestId("nw-name").fill(`nb-${b}`);
  });
  await expect(dialog.getByTestId("dialog-error")).toContainText("already exists");
  await tab.keyboard.press("Escape");
  expect(errors).toEqual([]);
});

test("a workspace from an issue is named after it and starts a chat with the issue as its draft", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const issue = ISSUES[info.project.name === "chromium" ? 0 : 1];
  const live = process.env.SWARM_E2E_LIVE!;
  // `swarm launch` needs a provider; the stand-in answers with the live chat.
  await tab.route("**/api/chat/new", (route) => route.fulfill({ json: { ok: true, session: live, agent: "orchestrator", pane: "fixture-chair" } }));
  const dialog = await newWorkspace(tab, "proj-gh", async () => {
    await tab.getByTestId("nw-mode-issue").click();
    await expect(tab.getByTestId("nw-issue")).toHaveCount(ISSUES.length);
    await tab.getByTestId("nw-issue").filter({ hasText: issue.title }).click();
    await expect(tab.getByTestId("nw-issue-body")).toContainText(issue.body);
  });
  await expect(dialog).toBeHidden();
  await expect(workspace(tab, `${issue.number}-${issue.title}`)).toBeVisible();
  await expect(tab.locator(`[data-testid="transcript"][data-chat="${live}"]`)).toBeVisible();
  await expect(tab.getByTestId("composer")).toHaveValue(`Issue #${issue.number}: ${issue.title}\n\n${issue.body}`);
  await tab.getByTestId("composer").fill("");
  expect(errors).toEqual([]);
});

test("the setup script streams its output; a failure keeps the workspace and says so", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const b = info.project.name;
  const setScript = async (script: string) => {
    await header(tab, "proj-gh").click({ button: "right" });
    await tab.getByRole("menuitem", { name: "Setup script…" }).click();
    const box = tab.getByTestId("setup-script");
    await expect(box).toBeEnabled();
    await box.fill(script);
    const saved = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "setup_script");
    await tab.getByTestId("setup-script-save").click();
    expect((await saved).status()).toBe(200);
  };
  try {
    // The marker file holds the script until the case lets it finish, so the live output shows first.
    const gate = join(process.env.SWARM_E2E_DIR!, `gate-${b}`);
    await setScript(`echo setup-started; while [ ! -e ${gate} ]; do sleep 0.1; done; echo to-stderr >&2; exit 3`);
    const dialog = await newWorkspace(tab, "proj-gh", async () => {
      await tab.getByTestId("nw-name").fill(`setup-${b}`);
      await expect(tab.getByTestId("new-workspace")).toContainText("The setup script runs in the new workspace");
    });
    const progress = dialog.getByTestId("nw-progress");
    await expect(dialog.getByTestId("setup-output")).toContainText("setup-started");
    await expect(progress).toHaveAttribute("data-state", "working");
    await expect(progress).toContainText("Running the setup script");
    await shoot(tab, "setup-output", b);
    writeFileSync(gate, "");
    await expect(progress).toHaveAttribute("data-state", "kept");
    await expect(dialog.getByTestId("setup-output")).toContainText("to-stderr");
    await expect(progress).toContainText("The setup script failed with exit code 3. The workspace was kept at");
    await expect(workspace(tab, `setup-${b}`)).toBeVisible();
    await dialog.getByTestId("nw-open").click();
    await expect(tab.getByTestId("workspace-landing")).toContainText(`setup-${b}`);

    // A script that succeeds closes the dialog on the new workspace.
    await setScript("echo all-good");
    await newWorkspace(tab, "proj-gh", async () => {
      await tab.getByTestId("nw-name").fill(`setup-ok-${b}`);
    });
    await expect(tab.getByTestId("new-workspace")).toBeHidden();
    await expect(tab.getByText("The setup script finished.")).toBeVisible();
    await expect(tab.getByTestId("workspace-landing")).toContainText(`setup-ok-${b}`);
  } finally {
    await setScript("");
  }
  expect(errors).toEqual([]);
});

test("archive asks while agents run and restores; delete asks and removes the folder", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const b = info.project.name;
  // proj-pr's main is on its origin, so a fresh branch there has nothing unpushed and may be deleted.
  await newWorkspace(tab, "proj-pr", async () => {
    await tab.getByTestId("nw-name").fill(`arch-${b}`);
  });
  const row = workspace(tab, `arch-${b}`);
  await expect(row).toBeVisible();
  const path = (await row.getAttribute("data-id"))!;
  const menu = async (item: string) => {
    await tab.locator(`[data-testid="tree-row"][data-id="${path}"]`).click({ button: "right" });
    await tab.getByRole("menuitem", { name: item }).click();
  };

  // The fixture workspace has no agent; the impact says one runs, so the confirm shows.
  await tab.route("**/api/workspace/impact*", async (route) => route.fulfill({ json: { ...(await (await route.fetch()).json()), liveAgents: 1, confirmationRequired: true } }), { times: 1 });
  await menu("Archive workspace");
  const confirm = tab.getByTestId("confirm");
  await expect(confirm).toContainText("Archive workspace?");
  await expect(confirm.getByTestId("confirm-body")).toHaveText("1 agent still running. Archiving stops them and keeps the workspace folder.");
  await confirm.getByTestId("confirm-action").click();
  await expect(confirm).toBeHidden();
  await expect(row).toBeHidden();
  expect(existsSync(path)).toBe(true);

  await tab.getByTestId("archive-toggle").click();
  await expect(tab.getByTestId("sidebar")).toHaveAttribute("aria-label", "Archived workspaces");
  await menu("Restore workspace");
  await expect(tab.locator(`[data-testid="tree-row"][data-id="${path}"]`)).toBeHidden();
  await tab.getByTestId("archive-toggle").click();
  await expect(row).toBeVisible();

  await menu("Delete workspace…");
  await expect(confirm).toContainText("Delete workspace?");
  await expect(confirm.getByTestId("confirm-body")).toContainText(`Delete the workspace folder at ${path}?`);
  await shoot(tab, "delete-confirm", b);
  await confirm.getByTestId("confirm-action").click();
  await expect(confirm).toBeHidden();
  await expect(row).toBeHidden();
  expect(existsSync(path)).toBe(false);
  expect(errors).toEqual([]);
});

test("Archive chat moves a chat to Recently closed, and ⇧⌘T reopens it", async ({ page: tab }) => {
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
    await archived.locator('[data-action="unarchive"]').click();
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

test("Switch model shows its progress, and Cancel stops it while the chair writes its summary", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const live = process.env.SWARM_E2E_LIVE!;
  const chats = async () => (await readTree()).sections.flatMap((s) => s.rows).filter((r) => r.kind === "chat").length;
  const before = await chats();
  await tab.locator(`[data-testid="tree-row"][data-session="${live}"]`).click();
  await tab.getByTestId("model-chip").click();
  const dialog = tab.getByTestId("switch-model");
  await expect(dialog).toBeVisible();
  await dialog.getByTestId("sm-provider").selectOption("codex");
  await dialog.getByTestId("sm-query").fill("gpt-e2e");
  await dialog.getByTestId("sm-custom").click();
  await dialog.getByTestId("sm-start").click();
  const progress = dialog.getByTestId("sm-progress");
  // The live chair was asked for a summary and does not answer, so the switch waits here.
  await expect(progress).toHaveAttribute("data-phase", "summarizing");
  await shoot(tab, "switch-model", info.project.name);
  await dialog.getByTestId("sm-cancel").click();
  await expect(progress).toHaveAttribute("data-phase", "cancelled");
  await expect(progress).toContainText("Cancelled. This chat is unchanged.");
  expect(await chats()).toBe(before);
  await tab.keyboard.press("Escape");
  await expect(tab.locator(`[data-testid="transcript"][data-chat="${live}"]`)).toBeVisible();
  expect(errors).toEqual([]);
});

test("Import Project asks about a plain folder, and Remove Project hides it again", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const folder = join(process.env.SWARM_E2E_DIR!, `plain-${info.project.name}`);
  mkdirSync(folder, { recursive: true });
  await tab.getByRole("button", { name: "Add project" }).click();
  await tab.getByRole("menuitem", { name: "Import Project…" }).click();
  const dialog = tab.getByTestId("project-dialog");
  await dialog.getByTestId("project-path").fill(folder);
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
