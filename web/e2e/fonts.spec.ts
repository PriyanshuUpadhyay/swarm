import { expect, test } from "@playwright/test";
import { server } from "./settings-helpers";

test("the design fonts load from the app's own files, with every other origin blocked", async ({ page: tab }) => {
  const origin = server().origin;
  const blocked: string[] = [];
  await tab.route("**/*", (route) => {
    if (new URL(route.request().url()).origin === origin) return route.continue();
    blocked.push(route.request().url());
    return route.abort();
  });
  await tab.goto(`${origin}/${server().hash}`);
  await tab.waitForFunction(() => window.__swarmReady === true);
  // The serif has no user until the new-chat question shows, so ask for it as the page would.
  const status = await tab.evaluate(async () => {
    await document.fonts.load('16px "Instrument Serif"');
    await document.fonts.load('16px "DM Sans"');
    await document.fonts.load('13px "JetBrains Mono"');
    const out: Record<string, string[]> = {};
    for (const f of document.fonts) (out[f.family.replace(/"/g, "")] ??= []).push(f.status);
    return out;
  });
  for (const family of ["DM Sans", "JetBrains Mono", "Instrument Serif"]) {
    expect(status[family], family).toContain("loaded");
  }
  expect(blocked).toEqual([]);
});
