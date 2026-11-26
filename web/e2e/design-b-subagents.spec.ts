import { expect, test } from "@playwright/test";
import type { Row } from "../src/core/api";
import { open, row, rows } from "./ux-helpers";

test("a group shows observed states, expands after five and opens the chosen child", async ({ page }) => {
  const batch = {
    ...row("sub1", "Review the release", "subagent"),
    subagent: {
      call_id: "fixture-call", other_call_ids: ["fixture-call.2"], agent_type: "subagent", description: "", state: "finished" as const,
      children: [
        { call_id: "fixture-call", description: "Check rendering", state: "started" },
        { call_id: "fixture-call.2", description: "Check the docs", state: "done" },
        { call_id: "fixture-call.3", description: "Check the build", state: "failed" },
        { call_id: "fixture-call.4", description: "Check the version", state: "done" },
        { call_id: "fixture-call.5", description: "Check the test report" },
        { call_id: "fixture-call.6", description: "Check keyboard navigation", state: "future-state" },
      ],
    },
  } satisfies Row & { subagent: NonNullable<Row["subagent"]> & { children: unknown[] } };
  await page.route("**/api/rows?*", (route) => {
    const agent = new URL(route.request().url()).searchParams.get("agent");
    return route.fulfill({ json: rows(agent === "chair~sub~fixture-call.6" ? [row("a1", "Keyboard fixes are ready")] : [batch]) });
  });
  await open(page);
  const card = page.getByTestId("subagent-row");
  await expect(card).toContainText("1 failed");
  await expect(card).toContainText("2 unknown");
  await expect(card.getByRole("button").first()).toContainText("Check the build");
  const child = card.getByRole("button", { name: /Check keyboard navigation/ });
  await expect(child).toHaveCount(0);
  await card.getByRole("button", { name: "Show all", exact: true }).click({ timeout: 3000 });
  await expect(child).toBeVisible();
  if (process.env.DESIGN_B_SHOTS) for (const theme of ["dark", "light"] as const) {
    await page.emulateMedia({ colorScheme: theme });
    await card.screenshot({ path: `${process.env.DESIGN_B_SHOTS}/subagents-${theme}.png`, animations: "disabled" });
  }
  await child.click();
  await expect(page.getByTestId("subagent-chat")).toContainText("Keyboard fixes are ready");
});
