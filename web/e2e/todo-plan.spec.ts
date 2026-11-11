import { expect, test } from "@playwright/test";

// The server builds the rows (tests/view_rows); this spec checks how the page draws a to-do list
// and a plan row. The rows request answers with those two rows for the typed e2e chat.
test("a checklist row shows its steps and a plan row shows its Markdown", async ({ page: tab }) => {
  const live = process.env.SWARM_E2E_TYPED_CHAT!;
  const row = { collapsed: false, order: [0, 0] };
  const rows = [
    {
      ...row,
      id: "t1:call",
      kind: "todoList",
      text: "2 of 5 done",
      todo: [
        { text: "Read", status: "completed" },
        { text: "Plan", status: "completed" },
        { text: "Edit", status: "in_progress", active_text: "Editing the parser" },
        { text: "Test", status: "pending" },
        { text: "Ship", status: "pending" },
      ],
    },
    { ...row, id: "p1:call", kind: "proposedPlan", text: "## Steps\n\n1. Add the row", order: [1, 0] },
  ];
  await tab.route("**/api/rows?*", (route) => route.fulfill({ json: { page: { start_offset: 0, end_offset: 2 }, rows, usage: null } }));
  const url = new URL(process.env.SWARM_SERVE_URL!);
  await tab.goto(`${url.origin}/${url.hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  await tab.locator(`[data-testid="tree-row"][data-session="${live}"]`).click();

  const list = tab.getByTestId("todo-list");
  await expect(list).toContainText("2 of 5 done");
  await expect(list.locator("li")).toHaveCount(5);
  await expect(list.locator('li[data-status="in_progress"]')).toContainText("Editing the parser");
  await expect(tab.getByTestId("plan").locator("h2")).toHaveText("Steps");
  await tab.screenshot({ path: test.info().outputPath("todo-plan.png") });
});
