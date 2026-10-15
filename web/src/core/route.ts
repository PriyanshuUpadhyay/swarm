// The page URL names the open chat (ADR 0088): `/chat/<id>`, else `/`.

/** The chat id in `/chat/<id>`, or null for any other path. */
export function chatFromPath(path: string): string | null {
  const match = /^\/chat\/([^/]+)\/?$/.exec(path);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return null;
  }
}

export function chatPath(id: string | null): string {
  return id ? `/chat/${encodeURIComponent(id)}` : "/";
}

/** `id` when a session has it, else the one session whose id starts with it, so a short id such as
 *  `01a13ee5` opens its chat. Null when none or more than one match. */
export function resolveChat(id: string, ids: readonly string[]): string | null {
  if (ids.includes(id)) return id;
  const hits = ids.filter((s) => s.startsWith(id));
  return hits.length === 1 ? hits[0] : null;
}

/** The short chat id that the UI shows: the shortest start of `id`, at least 8 characters, that no
 *  other session shares. A UUID v7 starts with the time, so chats made within about a minute share
 *  their first 8 characters. */
export function shortId(id: string, ids: readonly string[]): string {
  let length = 8;
  while (length < id.length && ids.some((other) => other !== id && other.startsWith(id.slice(0, length)))) length++;
  return id.slice(0, length);
}
