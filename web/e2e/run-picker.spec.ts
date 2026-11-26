import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { open, readTree } from "./lifecycle-helpers";

async function shots(tab: Page, unit: number, name: string) {
  if (!process.env.E2E_SHOTS) return;
  const dir = resolve(import.meta.dirname, `../../tmp/shots/${unit}`);
  mkdirSync(dir, { recursive: true });
  for (const theme of ["dark", "light"]) {
    await tab.evaluate((value) => { document.documentElement.dataset.theme = value; }, theme);
    await tab.screenshot({ path: join(dir, `${name}-${theme}.png`) });
  }
  await tab.evaluate(() => { delete document.documentElement.dataset.theme; });
}

test("the same tab pages both CLI logs and reads tool output from its original session", async ({ page: tab }) => {
  const errors = await open(tab);
  const root = process.env.SWARM_E2E_GIANT!;
  const rootRow = tab.getByTestId("tree-row").and(tab.locator(`[data-session="${root}"]`));
  await rootRow.click();
  const priorTab = tab.getByTestId("tab").and(tab.locator(`[data-session="${root}"]`));
  await expect(priorTab).toBeVisible();
  const tabId = await priorTab.getAttribute("data-id");
  const rowId = await rootRow.getAttribute("data-id");

  const env = { PATH: process.env.PATH, HOME: process.env.SWARM_E2E_HOME, SWARM_HOME: process.env.SWARM_E2E_HOME, SWARM_ADAPTER: "tmux-swarm" };
  const swarm = process.env.SWARM_E2E_SWARM!;
  const dir = process.env.SWARM_E2E_DIR!;
  const call = (...args: string[]) => execFileSync(swarm, args, { cwd: join(dir, "ws-giant"), env, encoding: "utf8" }).trim().split("\n").at(-1)!;
  const leaf = call("session", "new", "lane", "--chair", `codex:${randomUUID()}`);
  const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
  const sql = (statement: string) => execFileSync("sqlite3", ["-cmd", ".timeout 5000", process.env.SWARM_E2E_DB!, statement]);
  const log = join(dir, `${leaf}.jsonl`);
  try {
    const line = { type: "response_item", payload: { type: "message", role: "user", content: [{ type: "input_text", text: "After the CLI switch" }] } };
    writeFileSync(log, `${JSON.stringify(line)}\n`);
    sql(`UPDATE session SET chair_log = ${quote(log)} WHERE id = ${quote(leaf)};
          INSERT INTO agent (id, session_id, role, provider, created_at, log) VALUES ('orchestrator', ${quote(leaf)}, 'orchestrator', 'codex', unixepoch(), ${quote(log)});`);
    call("session", "continue", leaf, root);
    await expect.poll(async () => (await readTree()).sections.flatMap((s) => s.rows).some((row) => row.selection?.chat === leaf)).toBe(true);
    await tab.reload();
    const row = tab.getByTestId("tree-row").and(tab.locator(`[data-session="${leaf}"]`));
    await expect(row).toHaveAttribute("data-id", rowId!);
    await row.click();
    await expect(tab.getByTestId("tab").and(tab.locator(`[data-session="${leaf}"]`))).toHaveAttribute("data-id", tabId!);
    const transcript = tab.getByTestId("transcript").and(tab.locator(`[data-chat="${leaf}"]`));
    await expect(transcript.getByText("After the CLI switch")).toBeVisible();
    const loadOlder = tab.getByTestId("load-older");
    await expect(loadOlder).toBeVisible();
    await loadOlder.click();
    await expect(transcript.getByText("Show the build log")).toBeVisible();
    await expect(transcript.getByTestId("model-change")).toHaveText("Claude → Codex");
    await shots(tab, 5, "linked-transcript");
    await transcript.getByTestId("fold").click();
    const card = transcript.getByTestId("tool-card").filter({ hasText: "cat big.log" });
    await card.getByRole("button", { name: /cat big\.log/ }).click();
    await card.getByTestId("show-full").click();
    await expect(card.getByTestId("full-output")).toContainText("line 0 of the build log");
  } finally {
    sql(`DELETE FROM agent WHERE session_id = ${quote(leaf)}; DELETE FROM session WHERE id = ${quote(leaf)};`);
    rmSync(log, { force: true });
    await expect.poll(async () => (await readTree()).sections.flatMap((s) => s.rows).some((row) => row.selection?.chat === root)).toBe(true);
  }
  expect(errors).toEqual([]);
});
