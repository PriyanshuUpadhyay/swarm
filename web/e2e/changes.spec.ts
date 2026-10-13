import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { GIANT_DIFF_LINES } from "./serve";

// The workspace panel (S4) on the git workspace from serve.ts: Changes in local and branch mode,
// the capped 20,000-line diff, line comments sent to the chat's agent, and the PR on a stand-in gh.

const home = () => process.env.SWARM_E2E_HOME!;
const read = (name: string) => {
  try {
    return readFileSync(join(home(), name), "utf8");
  } catch {
    return "";
  }
};

async function openGitChat(tab: Page) {
  const errors: string[] = [];
  tab.on("pageerror", (e) => !e.message.startsWith("ResizeObserver loop") && errors.push(e.message));
  tab.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  const session = process.env.SWARM_E2E_GIT!;
  await tab.locator(`[data-testid="tree-row"][data-session="${session}"]`).click();
  await expect(tab.locator(`[data-testid="tab"][data-session="${session}"]`)).toHaveAttribute("aria-selected", "true");
  return errors;
}

const rows = (tab: Page) => tab.getByTestId("change-row");
const row = (tab: Page, path: string) => tab.locator(`[data-testid="change-row"][data-path="${path}"]`);
const diffPath = (tab: Page) => tab.getByTestId("diff-path");

test("Changes lists local layers and branch commits, and keys move the file list", async ({ page: tab }) => {
  const errors = await openGitChat(tab);
  await tab.getByTestId("show-changes").click();
  await expect(tab.getByTestId("changes-list")).toHaveAttribute("data-state", "ready");
  await expect(row(tab, "README.md")).toHaveAttribute("data-layer", "unstaged");
  await expect(row(tab, "notes.md")).toHaveAttribute("data-layer", "staged");
  await expect(row(tab, "big.txt")).toHaveAttribute("data-layer", "untracked");
  await expect(rows(tab)).toHaveCount(3);
  await expect(tab.getByTestId("changes-summary")).toContainText("3 files");

  // The first file shows; j, End and Home move the selection and its diff follows.
  const paths = await rows(tab).evaluateAll((els) => els.map((e) => e.getAttribute("data-path")!));
  await expect(diffPath(tab)).toHaveText(paths[0]);
  await tab.getByTestId("changes-list").focus();
  await tab.keyboard.press("j");
  await expect(row(tab, paths[1])).toHaveAttribute("aria-selected", "true");
  await expect(diffPath(tab)).toHaveText(paths[1]);
  await tab.keyboard.press("End");
  await expect(diffPath(tab)).toHaveText(paths[2]);
  await tab.keyboard.press("Home");
  await expect(diffPath(tab)).toHaveText(paths[0]);

  // The read-only file preview of the unstaged README.
  await row(tab, "README.md").click();
  await expect(diffPath(tab)).toHaveText("README.md");
  await tab.getByTestId("show-file").click();
  await expect(tab.getByTestId("file-body")).toContainText("A test repository, changed.");
  await tab.getByRole("button", { name: "Diff", exact: true }).click();

  // Branch mode compares with origin/main: one commit that changed src/app.ts.
  await tab.getByTestId("mode-branch").click();
  await expect(tab.getByTestId("base-select")).toHaveValue("refs/remotes/origin/main");
  await expect(rows(tab)).toHaveCount(1);
  await expect(row(tab, "src/app.ts")).toHaveAttribute("data-layer", "branch");
  await expect(diffPath(tab)).toHaveText("src/app.ts");
  await expect(tab.getByTestId("diff-counts")).toHaveText("+3 −2");
  // The local main is the same commit as origin/main, so the list stays.
  await tab.getByTestId("base-select").selectOption("refs/heads/main");
  await expect(row(tab, "src/app.ts")).toBeVisible();
  await tab.getByTestId("mode-local").click();
  await expect(rows(tab)).toHaveCount(3);
  expect(errors).toEqual([]);
});

test("a 20,000-line diff opens capped and inside the 150 ms budget", async ({ page: tab }, info) => {
  const errors = await openGitChat(tab);
  await tab.getByTestId("show-changes").click();
  await expect(row(tab, "big.txt")).toBeVisible();
  const open = (path: string) =>
    tab.evaluate(
      (path) =>
        new Promise<number>((done) => {
          const start = performance.now();
          document.querySelector<HTMLElement>(`[data-testid="change-row"][data-path="${path}"]`)!.click();
          const tick = () => {
            const pane = document.querySelector('[data-testid="diff-pane"]');
            if (pane?.getAttribute("data-state") === "ready" && document.querySelector('[data-testid="diff-path"]')?.textContent === path) done(performance.now() - start);
            else requestAnimationFrame(tick);
          };
          tick();
        }),
      path,
    );
  const times: number[] = [];
  for (let i = 0; i < 5; i++) {
    await open("README.md");
    times.push(await open("big.txt"));
  }
  // The route's own share, so a miss shows whether the server or the page took the time.
  const route = await tab.evaluate(() => performance.getEntriesByType("resource").filter((e) => e.name.includes("path=big.txt")).map((e) => e.duration));
  info.annotations.push({ type: "big diff open ms", description: `click to paint ${times.map((t) => t.toFixed(0)).join(", ")}; route ${route.map((t) => t.toFixed(0)).join(", ")}` });
  expect(Math.max(...times), info.annotations.at(-1)!.description).toBeLessThanOrEqual(150);
  await expect(tab.getByTestId("diff-counts")).toHaveText(`+${GIANT_DIFF_LINES.toLocaleString()} −0`);
  expect(await tab.getByTestId("diff-line").count()).toBeLessThanOrEqual(200);
  await expect(tab.getByTestId("diff-capped")).toContainText("20,004 lines");
  expect(errors).toEqual([]);
});

test("a range comment and a line comment go to the chat's agent as one message", async ({ page: tab }, info) => {
  const errors = await openGitChat(tab);
  const who = info.project.name;
  await tab.getByTestId("show-changes").click();
  await tab.getByTestId("mode-branch").click();
  await expect(diffPath(tab)).toHaveText("src/app.ts");
  const line = (text: string) => tab.getByTestId("diff-line").filter({ hasText: text }).locator("[data-pick]").last();

  // A range: from the first removed line to the second added line, by click and Shift-click.
  await line("value10 = 10;").click();
  await line("value11 = 110;").click({ modifiers: ["Shift"] });
  await expect(tab.locator('[data-testid="diff-line"][data-picked]')).toHaveCount(4);
  await tab.getByTestId("comment-input").fill(`Range comment from ${who}`);
  await tab.keyboard.press("Enter");
  await expect(tab.getByTestId("comment-card")).toContainText(`Range comment from ${who}`);

  // One line in another file and mode; the first draft stays.
  await tab.getByTestId("mode-local").click();
  await row(tab, "README.md").click();
  await expect(diffPath(tab)).toHaveText("README.md");
  await line("A test repository, changed.").click();
  await tab.getByTestId("comment-input").fill(`Line comment from ${who}`);
  await tab.getByTestId("comment-add").click();
  await expect(tab.getByTestId("drafts")).toContainText("2 comments");
  await row(tab, "big.txt").click();
  await expect(diffPath(tab)).toHaveText("big.txt");
  await expect(tab.getByTestId("drafts")).toContainText("2 comments");

  const send = tab.getByTestId("send-comments");
  await expect(send).toBeEnabled();
  await send.click();
  const message = [
    "Review comments on the diff (2):",
    "",
    "src/app.ts (lines 10-11):",
    "> -export const value10 = 10;",
    "> -export const value11 = 11;",
    "> +export const value10 = 100;",
    "> +export const value11 = 110;",
    `Range comment from ${who}`,
    "",
    "README.md (line 3):",
    "> +A test repository, changed.",
    `Line comment from ${who}`,
  ].join("\n");
  await expect.poll(() => read("typed")).toContain(`${message}\n`);
  await expect(tab.getByTestId("drafts")).toBeHidden();
  expect(errors).toEqual([]);
});

test("the PR panel shows the branch's PR with passing, then failing checks", async ({ page: tab }) => {
  const dir = process.env.SWARM_E2E_GIT_DIR!;
  const git = (...args: string[]) => execFileSync("git", ["-C", dir, ...args], { encoding: "utf8" }).trim();
  const pr = (conclusion: string) => ({
    number: 42,
    title: "Add the feature",
    body: "",
    state: "OPEN",
    isDraft: false,
    baseRefName: "main",
    baseRefOid: git("rev-parse", "origin/main"),
    headRefName: "feature",
    headRefOid: git("rev-parse", "HEAD"),
    headRepository: { nameWithOwner: "example/project" },
    reviewDecision: "REVIEW_REQUIRED",
    url: "https://github.com/example/project/pull/42",
    statusCheckRollup: [
      { __typename: "CheckRun", name: "build", workflowName: "CI", status: "COMPLETED", conclusion, detailsUrl: "https://github.com/example/project/actions/runs/1" },
      { __typename: "StatusContext", context: "lint", state: "SUCCESS", targetUrl: "javascript:alert(1)" },
    ],
  });
  const serve = (conclusion: string) => {
    writeFileSync(join(home(), "pr-list"), JSON.stringify([pr(conclusion)]));
    writeFileSync(join(home(), "pr-view"), JSON.stringify(pr(conclusion)));
  };
  try {
    const errors = await openGitChat(tab);
    await tab.getByTestId("show-pr").click();
    const panel = tab.getByTestId("pr-panel");
    await expect(panel).toHaveAttribute("data-state", "none");

    serve("SUCCESS");
    await tab.getByTestId("panel-refresh").click();
    await expect(panel).toHaveAttribute("data-state", "ready");
    await expect(tab.getByTestId("pr-title")).toContainText("Add the feature #42");
    await expect(tab.getByTestId("pr-state")).toHaveText("Open");
    await expect(tab.getByTestId("checks-summary")).toHaveText("2 passed");
    // A link that is not http(s) is not a link.
    await expect(tab.getByTestId("check").filter({ hasText: "lint" }).getByRole("link")).toHaveCount(0);
    await expect(tab.getByTestId("check").filter({ hasText: "build" }).getByRole("link")).toHaveAttribute("href", "https://github.com/example/project/actions/runs/1");

    serve("FAILURE");
    await tab.getByTestId("panel-refresh").click();
    await expect(tab.getByTestId("checks-summary")).toHaveText("1 failed, 1 passed");
    await expect(tab.getByTestId("check").first()).toHaveAttribute("data-state", "failed");
    expect(errors).toEqual([]);
  } finally {
    rmSync(join(home(), "pr-list"), { force: true });
    rmSync(join(home(), "pr-view"), { force: true });
  }
});
