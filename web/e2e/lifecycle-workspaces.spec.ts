import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { ISSUES, seedEndedChat } from "./serve";
import { api, compose, header, holdChatStart, open, readTree, shoot, showOtherProjects } from "./lifecycle-helpers";

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
  await showOtherProjects(tab);
  const b = runName(info);
  const branch = `swarm/fix-flaky-smoke-${b}`;
  // The marker file holds the script until the case lets it fail, so the live output shows first.
  // A failed setup starts no chat (ADR 0097), so no agent launches in the shared home.
  const gate = join(process.env.SWARM_E2E_DIR!, `gate-${b}`);
  await setScript(tab, `echo setup-started; while [ ! -e ${gate} ]; do sleep 0.1; done; echo to-stderr >&2; exit 3`);
  try {
    const composer = await compose(tab, "proj-gh", `Fix the flaky smoke ${b}`);
    // The start names the branch, so the card only says where its name comes from (ADR 0158).
    await expect(composer.getByTestId("preview-branch")).toHaveText("named from the first prompt");
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
    // Edit and retry opens the composer again with the same prompt.
    await start.getByTestId("start-retry").click();
    await expect(tab.getByTestId("new-chat-prompt")).toHaveValue(`Fix the flaky smoke ${b}`);
    await tab.getByTestId("new-chat-prompt").press("Escape");

    const path = execFileSync("git", ["-C", ghProject(), "worktree", "list", "--porcelain"], { encoding: "utf8" })
      .split("\n\n")
      .find((block) => block.includes(`branch refs/heads/${branch}`))
      ?.match(/^worktree (.*)$/m)?.[1] ?? null;
    expect(path).not.toBeNull();
  } finally {
    await setScript(tab, "");
    // Leave the registered worktree until teardown. Removing it through Git leaves the
    // server's workspace selection pointing at a missing folder in the next test.
  }
  expect(errors).toEqual([]);
});

test("the place menu sends a branch, a pull request or an issue as the chat's place", async ({ page: tab }, info) => {
  const errors = await open(tab);
  await showOtherProjects(tab);
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

test("an owner fold does not wait for a branch's GitHub lookup", async () => {
  const dir = process.env.SWARM_E2E_DIR!;
  const home = process.env.SWARM_E2E_HOME!;
  const cwd = process.env.SWARM_E2E_GIT_DIR!;
  const gh = join(dir, "bin/gh");
  const original = readFileSync(gh, "utf8");
  const replaceGh = (body: string) => {
    writeFileSync(`${gh}.next`, body, { mode: 0o755 });
    renameSync(`${gh}.next`, gh);
  };
  const before = await readTree();
  const project = before.sections.find((s) => s.path === realpathSync(cwd))!;
  expect(project.expanded).toBe(true);
  let folded = false;
  const key = randomUUID();
  const marker = join(home, `fold-gh-started-${key}`);
  const release = join(home, `fold-gh-release-${key}`);
  const git = (...args: string[]) => execFileSync("git", ["-C", cwd, ...args], { stdio: "pipe" });
  const branch = `fold-gh-${key}`;
  try {
    replaceGh(original.replace("#!/bin/sh\n", `#!/bin/sh
if [ "$1 $2" = "repo view" ]; then
  touch "$HOME/fold-gh-started-${key}"
  for delay in 1 2 3 4 5 6 7 8; do
    [ ! -e "$HOME/fold-gh-release-${key}" ] || break
    sleep 1
  done
fi
`));
    git("switch", "-c", branch);
    await expect.poll(() => existsSync(marker), { timeout: 18_000 }).toBe(true);
    const tree = await readTree();
    const url = new URL(process.env.SWARM_SERVE_URL!);
    const token = new URLSearchParams(url.hash.slice(1)).get("token");
    const response = await fetch(`${url.origin}/api/choices`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({ revision: tree.revision, op: "collapse", id: project.collapseId }),
      signal: AbortSignal.timeout(2_000),
    });
    folded = response.ok;
    expect(response.status).toBe(200);
  } finally {
    writeFileSync(release, "");
    replaceGh(original);
    git("switch", "feature");
    git("branch", "-D", branch);
    rmSync(marker, { force: true });
    if (folded) {
      const { revision } = await readTree();
      await api("/api/choices", { revision, op: "expand", id: project.collapseId });
    }
  }
});

test("archive asks while agents run and restores; delete asks and removes the folder", async ({ page: tab }, info) => {
  const b = runName(info);
  // proj-pr's main is on its origin, so a fresh branch there has nothing unpushed and may be deleted.
  const { workspace: path } = await api<{ workspace: string }>("/api/workspace/new", {
    project: realpathSync(join(process.env.SWARM_E2E_DIR!, "proj-pr")), name: `arch-${b}`, mode: "new-branch",
  });

  // An empty worktree has no sidebar row. Give it one ended fixture chat.
  const home = process.env.SWARM_E2E_HOME!;
  const chair = randomUUID();
  const logs = join(home, ".claude/projects/e2e");
  const log = join(logs, `${chair}.jsonl`);
  copyFileSync(join(import.meta.dirname, "../../tests/fixtures/provider-logs/claude-edit.jsonl"), log);
  const session = seedEndedChat(path, log, {
    PATH: process.env.PATH, HOME: home, SWARM_HOME: home, CLAUDE_CONFIG_DIR: join(home, ".claude"),
  });

  const errors = await open(tab);
  const row = tab.locator(`[data-testid="tree-row"][data-session="${session}"]`);
  const archived = tab.locator(`[data-testid="tree-row"][data-id="${path}"]`);
  await expect(row).toBeVisible();

  const menu = async (item: string) => {
    await (item === "Restore workspace" ? archived : row).click({ button: "right" });
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
  expect(existsSync(path)).toBe(false);
  await api("/api/sessions/archive", { ids: [session] });
  await expect(row).toBeHidden();
  expect(errors).toEqual([]);
});
