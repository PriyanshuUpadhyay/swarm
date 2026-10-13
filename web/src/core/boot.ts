import { Api, takeToken } from "./api";
import { App } from "./app";

/** Returns the app, or the text to show when the page has no token. */
export function boot(): App | string {
  const token = takeToken(location, history);
  if (!token) return "Open the link that swarm serve printed. The link holds the access token.";
  return new App(new Api(token), new URLSearchParams(location.search).get("live") === "all");
}
