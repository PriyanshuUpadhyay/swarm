import { expect, test, type Page } from "@playwright/test";

// The sidebar and tab strip on the Rust view model (P2b). Each case puts the owner choices it
// changes back, because both browsers share one server home.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;

/** The tree as the server has it now, read outside the page. */
async function readTree() {
  const res = await fetch(`${server().origin}/api/tree?view=workspaces`, { headers: { Authorization: `Bearer ${token()}` } });
  return (await res.json()) as { sections: { kind: string; rows: { id: string; kind: string; title: string; pinned: boolean }[] }[] };
}

async function open(tab: Page) {
  const errors: string[] = [];
  // WebKit's ResizeObserver loop report at load is not an error (see smoke.spec.ts).
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  // A refused choice logs its response as a console error; the stale case expects one.
  tab.on("console", (m) => m.type() === "error" && !m.text().includes("409") && errors.push(m.text()));
  await tab.goto(`${server().origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  return errors;
}

const workspaceRow = (tab: Page, name: string) => tab.locator('[data-testid="tree-row"][data-kind="workspace"]').filter({ hasText: name });

/** Opens a chat from the sidebar and waits for its tab to be the selected one. */
async function openChat(tab: Page, session: string) {
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.locator(`[data-testid="tab"][data-session="${session}"]`)).toHaveAttribute("aria-selected", "true");
}

test("a workspace dragged onto the Pinned header is pinned; the menu pins and unpins", async ({ page: tab }) => {
  const errors = await open(tab);
  const pinnedIds = async () => (await readTree()).sections.find((s) => s.kind === "pinned")?.rows.filter((r) => r.kind === "workspace").map((r) => r.id) ?? [];
  const live = (await workspaceRow(tab, "ws-live").getAttribute("data-id"))!;
  const build = (await workspaceRow(tab, "ws-build").getAttribute("data-id"))!;
  const menu = async (path: string, item: string) => {
    await tab.locator(`[data-testid="tree-row"][data-id="${path}"]`).click({ button: "right" });
    await tab.getByRole("menuitem", { name: item }).click();
  };

  await menu(live, "Pin workspace");
  const header = tab.locator('[data-testid="tree-row"][data-kind="pinned"]');
  await expect(header).toBeVisible();
  await expect.poll(pinnedIds).toEqual([live]);

  await workspaceRow(tab, "ws-build").dragTo(header);
  await expect.poll(pinnedIds).toEqual(expect.arrayContaining([live, build]));
  // The sidebar lists it under Pinned, before the first project header.
  const order = () => tab.getByTestId("tree-row").evaluateAll((rows) => rows.map((r) => r.getAttribute("data-kind") === "project" ? "project" : r.getAttribute("data-id")));
  await expect.poll(async () => (await order()).indexOf(build) < (await order()).indexOf("project")).toBe(true);

  await menu(build, "Unpin workspace");
  await menu(live, "Unpin workspace");
  await expect(header).toBeHidden();
  await expect.poll(pinnedIds).toEqual([]);
  expect(errors).toEqual([]);
});

test("Rename workspace… in the context menu saves the name the server then lists", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const row = workspaceRow(tab, "ws-giant");
  const path = (await row.getAttribute("data-id"))!;
  const name = `Giant from ${info.project.name}`;
  await row.click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Rename workspace…" }).click();
  await tab.getByTestId("ask-name").fill(name);
  await tab.getByTestId("ask-save").click();

  const renamed = tab.locator(`[data-testid="tree-row"][data-id="${path}"]`);
  await expect(renamed).toContainText(name);
  await expect.poll(async () => (await readTree()).sections.flatMap((s) => s.rows).find((r) => r.id === path)?.title).toBe(name);

  // An empty name clears the saved one.
  await renamed.click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Rename workspace…" }).click();
  await tab.getByTestId("ask-name").fill("");
  await tab.getByTestId("ask-save").click();
  await expect(renamed).not.toContainText(name);
  expect(errors).toEqual([]);
});

test("⌘2 picks the second tab in strip order, after a drag reorders the strip", async ({ page: tab }) => {
  const errors = await open(tab);
  const chats = workspaceRow(tab, "ws-tabs").locator("xpath=following-sibling::*[@data-kind='chat']");
  const [a, b] = await Promise.all([0, 1].map(async (i) => (await chats.nth(i).getAttribute("data-session"))!));
  await openChat(tab, a);
  await openChat(tab, b);
  const tabs = tab.getByTestId("tab");
  await expect(tabs).toHaveCount(2);
  const order = async () => tabs.evaluateAll((els) => els.map((e) => e.getAttribute("data-session")!));
  const transcript = (session: string) => tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`);

  const [first, second] = await order();
  await tab.keyboard.press("Meta+1");
  await expect(transcript(first)).toBeVisible();
  await tab.keyboard.press("Meta+2");
  await expect(transcript(second)).toBeVisible();

  // Drag the second tab onto the first: the strip order flips, and ⌘2 follows it.
  const moved = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "move_tab");
  await tab.locator(`[data-testid="tab"][data-session="${second}"]`).dragTo(tab.locator(`[data-testid="tab"][data-session="${first}"]`));
  expect((await moved).status()).toBe(200);
  await expect.poll(order).toEqual([second, first]);
  await tab.keyboard.press("Meta+2");
  await expect(transcript(first)).toBeVisible();
  expect(errors).toEqual([]);
});

test("⌃Tab goes back in tab history and ⌃⇧Tab forward", async ({ page: tab }) => {
  const errors = await open(tab);
  const chats = workspaceRow(tab, "ws-tabs").locator("xpath=following-sibling::*[@data-kind='chat']");
  const [a, b] = await Promise.all([0, 1].map(async (i) => (await chats.nth(i).getAttribute("data-session"))!));
  const transcript = (session: string) => tab.locator(`[data-testid="transcript"][data-chat="${session}"][data-state="ready"]`);
  await openChat(tab, b);
  await openChat(tab, a);
  // The server's history has both opens once its "previous recent" names the first.
  const key = (session: string) => tab.locator(`[data-testid="tab"][data-session="${session}"]`).getAttribute("data-id");
  await expect(tab.getByTestId("tabs")).toHaveAttribute("data-previous-recent", (await key(b))!);
  await tab.keyboard.press("Control+Tab");
  await expect(transcript(b)).toBeVisible();
  await expect(tab.getByTestId("tabs")).toHaveAttribute("data-next-recent", (await key(a))!);
  await tab.keyboard.press("Control+Shift+Tab");
  await expect(transcript(a)).toBeVisible();
  expect(errors).toEqual([]);
});

test("a stale revision shows the error and reloads the tree", async ({ page: tab }) => {
  const errors = await open(tab);
  // Another writer changed the choices: the page's revision no longer matches.
  await tab.route("**/api/choices", (route) => route.continue({ postData: JSON.stringify({ ...route.request().postDataJSON(), revision: "0".repeat(64) }) }), { times: 1 });
  const row = workspaceRow(tab, "ws-build");
  await row.click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Rename workspace…" }).click();
  await tab.getByTestId("ask-name").fill("never saved");
  const refused = tab.waitForResponse((r) => r.url().endsWith("/api/choices"));
  const reread = tab.waitForRequest((r) => r.url().includes("/api/tree?"));
  await tab.getByTestId("ask-save").click();
  expect((await refused).status()).toBe(409);
  await reread;
  await expect(tab.getByText("Your choices changed elsewhere. The list was reloaded; try again.")).toBeVisible();
  await expect(row).not.toContainText("never saved");
  expect((await readTree()).sections.flatMap((s) => s.rows).some((r) => r.title === "never saved")).toBe(false);
  expect(errors).toEqual([]);
});

test("⌘K finds a chat by a fuzzy query and opens it", async ({ page: tab }) => {
  const errors = await open(tab);
  const giant = (await tab.locator('[data-testid="tree-row"][data-kind="chat"]').filter({ hasText: "Show the build log" }).first().getAttribute("data-session"))!;
  await tab.getByTestId("sidebar").focus();
  await tab.keyboard.press("Meta+k");
  // Dropped letters and a missing word still find "Show the build log".
  await tab.getByPlaceholder("Search actions, workspaces, chats, and agents").fill("buld lg");
  const option = tab.getByRole("option").filter({ hasText: "Show the build log" }).first();
  await expect(option).toBeVisible();
  await option.click();
  await expect(tab.locator(`[data-testid="transcript"][data-chat="${giant}"][data-state="ready"]`)).toBeVisible();
  expect(errors).toEqual([]);
});

test("Recent lists chats across projects and opens one in another project from the keyboard", async ({ page: tab }) => {
  const errors = await open(tab);
  const sessionIn = async (ws: string) => (await workspaceRow(tab, ws).locator("xpath=following-sibling::*[@data-kind='chat']").first().getAttribute("data-session"))!;
  const build = await sessionIn("ws-build");
  const edit = await sessionIn("ws-edit");
  await openChat(tab, build);
  await expect(tab.getByTestId("tab-strip")).toContainText("ws-build");

  await tab.keyboard.press("Alt+Meta+2");
  const rows = tab.getByTestId("recent-row");
  await expect(tab.locator(`[data-testid="recent-row"][data-session="${build}"]`)).toHaveAttribute("aria-current", "true");
  const order = await rows.evaluateAll((els) => els.map((e) => e.getAttribute("data-session")));
  const target = order.indexOf(edit);
  expect(target).toBeGreaterThanOrEqual(0);
  await tab.keyboard.press("Home");
  for (let i = 0; i < target; i++) await tab.keyboard.press("j");
  await expect(tab.locator(`[data-testid="recent-row"][data-session="${edit}"]`)).toHaveAttribute("aria-selected", "true");
  await tab.keyboard.press("Enter");
  await expect(tab.locator(`[data-testid="transcript"][data-chat="${edit}"][data-state="ready"]`)).toBeVisible();
  await expect(tab.getByTestId("tab-strip")).toContainText("ws-edit");
  await expect(tab.locator(`[data-testid="recent-row"][data-session="${edit}"]`)).toHaveAttribute("aria-current", "true");
  await tab.keyboard.press("Alt+Meta+1");
  expect(errors).toEqual([]);
});
