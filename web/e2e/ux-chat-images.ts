import { expect, test } from "@playwright/test";
import { mockRows, mockStream, open, row, rows, streamFrame } from "./ux-helpers";
import type { PatchInfo, Row, TextPreview } from "../src/core/api";

// The served paths differ from the raw text, so a browser reparse cannot pass.
test("images paint served paths on pages, older rows, upserts and snapshot queues", async ({ page: tab }, info) => {
  const a = "/tmp/uploads/00000000-0000-0000-0000-000000000001.png";
  const b = "/tmp/uploads/00000000-0000-0000-0000-000000000002.webp";
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg==", "base64");
  const requested: string[] = [];
  await tab.route("**/api/chat/image?*", route => {
    requested.push(new URL(route.request().url()).searchParams.get("name")!);
    return route.fulfill({ contentType: "image/png", body: png });
  });
  await tab.route("**/api/rows?*", route => {
    const older = new URL(route.request().url()).searchParams.has("before");
    return route.fulfill({ json: older
      ? { ...rows([row("u0", a, "user")]), images: { u0: [b] } }
      : { ...rows([row("u1", a, "user"), row("q2", a, "queued")], 100), images: { u1: [b, a], q2: [b] } } });
  });
  let queueImages = [b];
  await tab.route("**/api/snapshot", async route => {
    const snapshot = await (await route.fetch()).json();
    snapshot.agents[process.env.SWARM_E2E_LIVE!][0].queue = [
      { mode: "steering", text: a, id: "served-queue", detail: "Steering", display: null, body: "plain", images: queueImages },
    ];
    await route.fulfill({ json: snapshot });
  });
  await mockStream(tab, "s8d");
  await open(tab);
  const names = (id: string) => tab.locator(`[data-testid="row"][data-id="${id}"]`).getByTestId("image-thumb").locator("img");
  const previews = (id: string, expected: string[]) => expect.poll(() => names(id).evaluateAll(els => els.map(el => el.getAttribute("alt")))).toEqual(expected);
  const aName = a.slice(a.lastIndexOf("/") + 1);
  const bName = b.slice(b.lastIndexOf("/") + 1);
  await previews("u1", [bName, aName]);
  await previews("q2", [bName]);
  await previews("served-queue", [bName]);
  // History may load on first paint. The route reply is the same for the button path.
  const earlier = tab.getByTestId("load-older");
  if (await earlier.isVisible()) await earlier.click();
  await previews("u0", [bName]);
  const key = `${process.env.SWARM_E2E_LIVE!}:chair`;
  const send = (upsert: ReturnType<typeof row>[], images?: Record<string, string[]>) => streamFrame(tab, "rows", { key, upsert, images, end_offset: 600 });
  await send([row("u1", a, "user")], { u1: [b] });
  await previews("u1", [bName]);
  await send([row("u1", a, "user")]);
  await previews("u1", []);
  queueImages = [b, a];
  await streamFrame(tab, "snapshot", {});
  await previews("served-queue", [bName, aName]);
  expect([...new Set(requested)].sort()).toEqual([aName, bName]);
  await tab.screenshot({ path: info.outputPath("s8d-served-images.png") });
});

test("the ⌘N composer takes a pasted image and holds Start until it is stored", async ({ page: tab }) => {
  let release!: () => void;
  const wait = new Promise<void>((r) => { release = r; });
  await tab.route("**/api/chat/image", async (route) => { await wait; await route.fulfill({ json: { path: "/tmp/uploads/12345678-1234-1234-1234-123456789abc.png" } }); });
  await open(tab);
  await tab.keyboard.press("Meta+n");
  const overlay = tab.getByTestId("composer-overlay");
  const prompt = overlay.getByTestId("new-chat-prompt");
  await expect(prompt).toBeFocused();
  await prompt.fill("Look at this");
  await prompt.evaluate((el: HTMLTextAreaElement) => {
    el.setSelectionRange(el.value.length, el.value.length);
    const data = new DataTransfer();
    data.items.add(new File(["image"], "screen.png", { type: "image/png" }));
    el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true }));
  });
  await expect(prompt).toHaveValue(/Uploading image/);
  await expect(overlay.getByTestId("new-chat-start")).toBeDisabled();
  await tab.keyboard.insertText(" please");
  release();
  await expect(prompt).toHaveValue(/^Look at this .*123456789abc\.png.* please$/);
  await expect(overlay.getByRole("button", { name: "Remove image", exact: true })).toBeAttached();
  await tab.keyboard.press("Escape");
});

test("a shown chunk diff opens outside its closed fold with the whole patch in Unified and Split", async ({ page: tab }) => {
  const tool = (id: string, command: string, output: TextPreview): Row => ({
    ...row(id, command, "toolUse"),
    tool: { name: "Bash", input: { command }, command, output, state: "finished", header_title: command },
  });
  const header = "--- a.ts\n+++ a.ts.new\n@@ -1,40 +1,40 @@";
  const full = `${header}\n-old line\n+new line\n${Array.from({ length: 39 }, (_, i) => ` context ${i + 2}`).join("\n")}`;
  const chunk: TextPreview = { id: "chunk-out", text: header, is_truncated: true, full_lines: 44, full_bytes: full.length, hidden_lines: 41 };
  const fold: Row = {
    ...row("f1", "2 commands", "fold"),
    default_open: false,
    fold: { commands: 2, waits: 0, rings: 0, failed: 0, interrupted: 0, unreported: 0, latest_title: "cat", state: "finished", text: "Ran 2 commands" },
    steps: [tool("s1", "ls", { id: "ls-out", text: "a.ts", full_lines: 1, full_bytes: 4, hidden_lines: 0 }), tool("s2", "cat /tmp/pair/chunk-9.diff", chunk)],
  };
  const layout = (text: string): PatchInfo => {
    const kinds = { "-": "del", "+": "add", "@": "hunk" } as const;
    const lines = text.split("\n").map((t, i) => ({ kind: i < 2 ? "meta" : (kinds[t[0] as keyof typeof kinds] ?? "ctx"), text: i < 3 ? t : t.slice(1), ...(i === 3 && { old: 1 }), ...(i === 4 && { new: 1 }), ...(i > 4 && { old: i - 3, new: i - 3 }) })) as Extract<PatchInfo, { lines: unknown }>["lines"];
    const split = lines.length > 3 ? [{ h: 2 }, { l: 3, r: 4 }, ...lines.slice(5).map((_, i) => ({ l: i + 5, r: i + 5 }))] : [{ h: 2 }];
    return { lines, split };
  };
  await mockRows(tab, [fold], 0, { "chunk-out": layout(chunk.text) });
  const asked: string[] = [];
  await tab.route("**/api/tool-output?*", (route) => {
    asked.push(new URL(route.request().url()).searchParams.get("diff") ?? "");
    return route.fulfill({ json: { text: full, patch: layout(full) } });
  });
  await open(tab);

  const shown = tab.getByTestId("shown-diff");
  await expect(shown).toContainText("new line");
  await expect(shown.locator(".diff-head")).toContainText("44 lines");

  const box = shown.locator(".patch");
  expect(await box.evaluate((el) => el.clientHeight)).toBeLessThanOrEqual(360);
  await box.evaluate((el) => { el.scrollTop = el.scrollHeight; });
  await expect(box.getByText("context 40", { exact: true })).toBeInViewport();
  await box.evaluate((el) => { el.scrollTop = 0; });
  expect(asked).toEqual(["true"]);
  await expect(tab.getByText("a.ts", { exact: true })).toBeHidden();

  await shown.getByRole("button", { name: "Split", exact: true }).click();
  await expect(shown).toHaveAttribute("data-split", "true");
  await expect(shown).toContainText("old line");

  if (process.env.DESIGN_B_SHOTS) for (const theme of ["dark", "light"] as const) {
    await tab.emulateMedia({ colorScheme: theme });
    await tab.evaluate((theme) => { document.documentElement.dataset.theme = theme; }, theme);
    await shown.screenshot({ path: `${process.env.DESIGN_B_SHOTS}/shown-diff-${theme}.png`, animations: "disabled" });
  }
});
