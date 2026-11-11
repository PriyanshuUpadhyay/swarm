import { expect, test } from "@playwright/test";
import type { Row } from "../src/core/api";
import { mockRows, open, row } from "./ux-helpers";

// The server builds the rows (tests/view_rows); this spec checks how the page draws a to-do list
// and a plan row.
test("a checklist row shows its steps and a plan row shows its Markdown", async ({ page: tab }) => {
  const rows: Row[] = [
    {
      ...row("t1", "2 of 5 done", "todoList"),
      todo: [
        { text: "Read", status: "completed" },
        { text: "Plan", status: "completed" },
        { text: "Edit", status: "in_progress", active_text: "Editing the parser" },
        { text: "Test", status: "pending" },
        { text: "Ship", status: "pending" },
      ],
    },
    row("p1", "## Steps\n\n1. Add the row", "proposedPlan"),
  ];

  await mockRows(tab, rows);
  await open(tab, process.env.SWARM_E2E_TYPED_CHAT!);

  const list = tab.getByTestId("todo-list");
  await expect(list).toContainText("2 of 5 done");
  await expect(list.locator("li")).toHaveCount(5);
  await expect(list.locator('li[data-status="in_progress"]')).toContainText("Editing the parser");
  await expect(list.locator('li[data-status="pending"]').first().getByRole("img", { name: "Pending" })).toBeVisible();
  await expect(tab.getByTestId("plan").locator("h2")).toHaveText("Steps");
});
