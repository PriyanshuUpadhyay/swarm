import { Api, takeToken } from "./api";
import { App } from "./app";

export const NO_TOKEN = "Open the link that swarm ui printed. The link holds the access token.";

/** The page's local storage, or undefined where the browser blocks it. */
export function savedStore(): Storage | undefined {
  try {
    return localStorage;
  } catch {
    return undefined;
  }
}

/** Returns the app, or the text to show when the page has no token. */
export function boot(): App | string {
  const token = takeToken(location, history, savedStore());
  if (!token) return NO_TOKEN;
  return new App(new Api(token), new URLSearchParams(location.search).get("live") === "all");
}
