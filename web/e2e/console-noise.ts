import type { ConsoleMessage } from "@playwright/test";

/** A reload or a second `goto` cuts off Pierre's language and theme imports, and Pierre logs the
 *  failed import while the page goes away. The owner never sees it. Our own lazy imports log a
 *  warning on failure, so this skips only Pierre's log. */
export const pierreUnloadNoise = (m: ConsoleMessage) =>
  m.text() === "TypeError: Importing a module script failed." && /\/assets\/pierre-patch-[^/]+\.js$/.test(m.location().url);
