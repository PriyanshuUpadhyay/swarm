import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ISSUES } from "./serve";
import { header, holdChatStart, newWorkspace, open, shoot, workspace } from "./lifecycle-helpers";

// The cases share one server home with every spec, and a repeat finds the workspaces of the run
// before, so each name carries the browser and the repeat.
const runName = (info: { project: { name: string }; repeatEachIndex: number }) => `${info.project.name}-${info.repeatEachIndex}`;
const ghProject = () => join(process.env.SWARM_E2E_DIR!, "proj-gh");

test("New workspace makes a new branch, an existing branch and a pull request workspace", async ({ page: tab }, info) => {
  const errors = await open(tab);
  await holdChatStart(tab);
  const b = runName(info);
  execFileSync("git", ["-C", ghProject(), "branch", `topic-${b}`]);
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

test("a workspace from an issue is named after it and starts a chat with the issue as its draft", async ({ page: tab }) => {
  const errors = await open(tab);
  const issue = ISSUES[0];
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
  const made = workspace(tab, `${issue.number}-${issue.title}`);
  await expect(made).toBeVisible();
  try {
    await expect(tab.locator(`[data-testid="transcript"][data-chat="${live}"]`)).toBeVisible();
    await expect(tab.getByTestId("composer")).toHaveValue(`Issue #${issue.number}: ${issue.title}\n\n${issue.body}`);
    await tab.getByTestId("composer").fill("");
  } finally {
    // The issue names the workspace, so a repeat needs its folder and branch gone. Delete refuses
    // here, because proj-gh's origin has no remote refs and so every commit counts as unpushed.
    const path = (await made.getAttribute("data-id"))!;
    const branch = execFileSync("git", ["-C", path, "branch", "--show-current"], { encoding: "utf8" }).trim();
    execFileSync("git", ["-C", ghProject(), "worktree", "remove", "--force", path]);
    execFileSync("git", ["-C", ghProject(), "branch", "-D", branch]);
  }
  expect(errors).toEqual([]);
});

test("the setup script streams its output; a failure keeps the workspace and says so", async ({ page: tab }, info) => {
  const errors = await open(tab);
  await holdChatStart(tab);
  const b = runName(info);
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
  await holdChatStart(tab);
  const b = runName(info);
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
