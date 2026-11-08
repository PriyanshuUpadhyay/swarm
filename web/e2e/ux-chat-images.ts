import { expect, test } from "@playwright/test";
import { open, row, rows } from "./ux-helpers";

// HAND paths differ from text, so a browser reparse cannot pass this case.
test("S8d images paint served paths on pages, older rows, upserts and snapshot queues", async ({ page: tab }, info) => {
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
  await tab.route("**/api/snapshot", async route => {
    const snapshot = await (await route.fetch()).json();
    snapshot.agents[process.env.SWARM_E2E_LIVE!][0].queue = [
      { mode: "steering", text: a, id: "served-queue", detail: "Steering", display: null, images: [b] },
    ];
    await route.fulfill({ json: snapshot });
  });
  await tab.route("**/api/subscribe", route => route.fulfill({ json: { ok: true } }));
  await tab.addInitScript(() => {
    const fetch = window.fetch.bind(window);
    const scope = window as unknown as { s8dFrame?: (event: string, data: unknown) => void };
    window.fetch = async (input, init) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (!url.endsWith("/api/stream")) return fetch(input, init);
      const body = new ReadableStream<Uint8Array>({ start(controller) {
        scope.s8dFrame = (event, data) => controller.enqueue(new TextEncoder().encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
        scope.s8dFrame("hello", { stream: "s8d", build: "fixture" });
      } });
      return new Response(body, { headers: { "Content-Type": "text/event-stream" } });
    };
  });
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
  const send = (upsert: ReturnType<typeof row>[], images?: Record<string, string[]>) => tab.evaluate(({ key, upsert, images }) => {
    const scope = window as unknown as { s8dFrame: (event: string, data: unknown) => void };
    scope.s8dFrame("rows", { key, upsert, images, end_offset: 600 });
  }, { key, upsert, images });
  await send([row("u1", a, "user")], { u1: [b] });
  await previews("u1", [bName]);
  await send([row("u1", a, "user")]);
  await previews("u1", []);
  expect([...new Set(requested)].sort()).toEqual([aName, bName]);
  await tab.screenshot({ path: info.outputPath("s8d-served-images.png") });
});
