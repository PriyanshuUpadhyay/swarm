import { expect, test, type Page } from "@playwright/test";

// ADR 0102: Home shows the notice card with its fix, and a notice click opens its chat.

const server = () => new URL(process.env.SWARM_SERVE_URL!);
const token = () => new URLSearchParams(server().hash.slice(1)).get("token")!;

async function ready(tab: Page) {
  await tab.waitForFunction(() => window.__swarmReady === true);
}

test("a held notice shows its steps on Home, and a test that works says so", async ({ page: tab }) => {
  const errors: string[] = [];
  tab.on("pageerror", (e) => errors.push(e.message));
  const held = { state: "held", reason: "A Focus held swarm's last notice.", steps: ["Click Open Focus settings."], action: "focus" };
  await tab.route("**/api/notices", (route) =>
    route.fulfill({ json: route.request().method() === "GET" ? held : { state: "ok", reason: "", steps: [], action: null } }),
  );
  await tab.goto(`${server().origin}/${server().hash}`);
  await ready(tab);
  // The shared home can reopen the last chat, so go to Home first.
  await tab.getByTestId("home-button").click();
  const card = tab.getByTestId("notices-card");
  await expect(card.getByRole("heading")).toHaveText("A Focus is holding swarm's notices");
  await expect(card.getByRole("listitem")).toHaveText("Click Open Focus settings.");
  await card.getByRole("button", { name: "Send a test notice" }).click();
  await expect(card.getByRole("status")).toHaveText(/Notices work/);
  await expect(card.getByRole("button", { name: "Done" })).toBeFocused();
  await card.getByRole("button", { name: "Done" }).click();
  await expect(card).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("a notice click's show frame opens that chat in the open page", async ({ page: tab, request }) => {
  await tab.goto(`${server().origin}/${server().hash}`);
  await ready(tab);
  const session = (await tab.locator('[data-testid="tree-row"][data-kind="chat"]').first().getAttribute("data-session"))!;
  const answer = await request.post(`${server().origin}/api/show`, { headers: { Authorization: `Bearer ${token()}` }, data: { session } });
  expect(answer.ok()).toBe(true);
  await expect(tab).toHaveURL(`${server().origin}/chat/${session}`);
  const bad = await request.post(`${server().origin}/api/show`, { headers: { Authorization: `Bearer ${token()}` }, data: { session: "../x" } });
  expect(bad.status()).toBe(400);
});
