import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, realpathSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ISSUES } from "./serve";
import { api, compose, header, holdChatStart, open, shoot, workspace } from "./lifecycle-helpers";

// The cases share one server home with every spec, and a repeat finds the workspaces of the run
// before, so each name carries the browser and the repeat.
const runName = (info: { project: { name: string }; repeatEachIndex: number }) => `${info.project.name}-${info.repeatEachIndex}`;
const ghProject = () => join(process.env.SWARM_E2E_DIR!, "proj-gh");

const setScript = async (tab: Page, script: string) => {
  await header(tab, "proj-gh").click({ button: "right" });
  await tab.getByRole("menuitem", { name: "Setup script…" }).click();
  const box = tab.getByTestId("setup-script");
  await expect(box).toBeEnabled();
  await box.fill(script);
  const saved = tab.waitForResponse((r) => r.url().endsWith("/api/choices") && r.request().postDataJSON().op === "setup_script");
  await tab.getByTestId("setup-script-save").click();
  expect((await saved).status()).toBe(200);
};

test("⌘Enter makes a worktree named from the prompt and shows each start step and the setup output", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const b = runName(info);
  const branch = `swarm/fix-flaky-smoke-${b}`;
  // The marker file holds the script until the case lets it fail, so the live output shows first.
  // A failed setup starts no chat (ADR 0097), so no agent launches in the shared home.
  const gate = join(process.env.SWARM_E2E_DIR!, `gate-${b}`);
  await setScript(tab, `echo setup-started; while [ ! -e ${gate} ]; do sleep 0.1; done; echo to-stderr >&2; exit 3`);
  let path: string | null = null;
  try {
    const composer = await compose(tab, "proj-gh", `Fix the flaky smoke ${b}`);
    // The card names the branch before the send.
    await expect(composer.getByTestId("preview-branch")).toHaveText(branch);
    await expect(composer.getByTestId("preview-setup")).toContainText("echo setup-started");
    await shoot(tab, "new-chat-composer", info.project.name);
    await composer.getByTestId("new-chat-prompt").press("Meta+Enter");

    const start = tab.getByTestId("starting-chat");
    await expect(start).toContainText(`Fix the flaky smoke ${b}`);
    await expect(start.getByTestId("start-steps")).toContainText(`Make the worktree: ${branch}`);
    await expect(start.getByTestId("setup-output")).toContainText("setup-started");
    await shoot(tab, "new-chat-starting", info.project.name);
    writeFileSync(gate, "");
    await expect(start).toHaveAttribute("data-state", "failed");
    await expect(start.getByTestId("setup-output")).toContainText("to-stderr");
    await expect(start.getByTestId("start-error")).toContainText("no chat started");
    path = execFileSync("git", ["-C", ghProject(), "worktree", "list", "--porcelain"], { encoding: "utf8" })
      .split("\n\n")
      .find((block) => block.includes(`branch refs/heads/${branch}`))
      ?.match(/^worktree (.*)$/m)?.[1] ?? null;
    expect(path).not.toBeNull();
  } finally {
    await setScript(tab, "");
    // proj-gh is no worktree hub, so a failed start keeps its worktree; a repeat needs both gone.
    if (path) execFileSync("git", ["-C", ghProject(), "worktree", "remove", "--force", path]);
    if (path) execFileSync("git", ["-C", ghProject(), "branch", "-D", branch]);
  }
  expect(errors).toEqual([]);
});

test("the place menu sends a branch, a pull request or an issue as the chat's place", async ({ page: tab }, info) => {
  const errors = await open(tab);
  const bodies = await holdChatStart(tab);
  const b = runName(info);
  execFileSync("git", ["-C", ghProject(), "branch", `topic-${b}`]);
  try {
    let composer = await compose(tab, "proj-gh", "Review the topic");
    await composer.getByTestId("chip-place").click();
    await composer.getByTestId("place-kind-branch").click();
    // The filter finds the branch in a long list, and Enter picks its first match.
    await composer.getByTestId("place-branch-filter").fill(`topic-${b}`);
    await composer.getByTestId("place-branch-filter").press("Enter");
    await expect(composer.getByTestId("chip-place")).toHaveText(`Branch topic-${b}`);
    await shoot(tab, "place-menu", info.project.name);
    await composer.getByTestId("new-chat-start").click();
    await expect.poll(() => bodies.length).toBe(1);
    expect(bodies[0]).toMatchObject({ prompt: "Review the topic", place: { kind: "branch", branch: `topic-${b}` } });
    expect(String(bodies[0].cwd)).toContain("/proj-gh");

    composer = await compose(tab, "proj-gh");
    await composer.getByTestId("chip-place").click();
    await composer.getByTestId("place-kind-pr").click();
    // An empty prompt is allowed; the PR number is not optional.
    await expect(composer.getByTestId("new-chat-start")).toBeDisabled();
    await composer.getByTestId("place-pr").fill("7");
    await composer.getByTestId("new-chat-prompt").press("Meta+Enter");
    await expect.poll(() => bodies.length).toBe(2);
    expect(bodies[1]).toMatchObject({ place: { kind: "pr", number: 7 } });
    expect(bodies[1].prompt).toBeUndefined();

    composer = await compose(tab, "proj-gh", "Fix it");
    await composer.getByTestId("chip-place").click();
    await composer.getByTestId("place-kind-issue").click();
    await expect(composer.getByTestId("place-issue")).toHaveCount(ISSUES.length);
    await composer.getByTestId("place-issue").filter({ hasText: ISSUES[0].title }).click();
    await expect(composer.getByTestId("chip-place")).toHaveText(`Issue #${ISSUES[0].number}`);
    await composer.getByTestId("new-chat-start").click();
    await expect.poll(() => bodies.length).toBe(3);
    expect(bodies[2]).toMatchObject({ prompt: "Fix it", place: { kind: "issue", number: ISSUES[0].number } });
  } finally {
    execFileSync("git", ["-C", ghProject(), "branch", "-D", `topic-${b}`]);
  }
  expect(errors).toEqual([]);
});

test("archive asks while agents run and restores; delete asks and removes the folder", async ({ page: tab }, info) => {
  const b = runName(info);
  // proj-pr's main is on its origin, so a fresh branch there has nothing unpushed and may be deleted.
  await api("/api/workspace/new", { project: realpathSync(join(process.env.SWARM_E2E_DIR!, "proj-pr")), name: `arch-${b}`, mode: "new-branch" });
  const errors = await open(tab);
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
  await expect(confirm.getByTestId("confirm-body")).toContainText("1 agent still running");
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

  await menu("Remove worktree…");
  await expect(confirm).toContainText("Remove worktree?");
  await expect(confirm.getByTestId("confirm-body")).toContainText(`Remove the worktree folder at ${path}? The branch stays`);
  await shoot(tab, "delete-confirm", info.project.name);
  await confirm.getByTestId("confirm-action").click();
  await expect(confirm).toBeHidden();
  await expect(row).toBeHidden();
  expect(existsSync(path)).toBe(false);
  expect(errors).toEqual([]);
});
