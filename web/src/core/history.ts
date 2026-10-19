// Sent-prompt history for Up and Down, the ⌘K Prompts group and Reuse. One list per workspace, so
// a new chat recalls the last chat's prompts. Pure, so vitest covers it.

export type HistoryEntry = { text: string; session: string; at: number };

/** Entries kept per workspace. */
export const HISTORY_CAP = 200;

/** Longer texts are not kept: 200 long pastes would pass the 5 MB that localStorage allows. */
export const HISTORY_TEXT_MAX = 10_000;

/** Appends a send, newest last. A repeat of the newest text from the same chat is kept once. */
export function recordSent(list: readonly HistoryEntry[], entry: HistoryEntry, cap = HISTORY_CAP): HistoryEntry[] {
  if (entry.text.length > HISTORY_TEXT_MAX) return [...list];
  const last = list.at(-1);
  if (last && last.text === entry.text && last.session === entry.session) return [...list];
  return [...list, entry].slice(-cap);
}

/** Texts newest first: this chat's, then the workspace's other chats', each text once. */
export function recallOrder(list: readonly HistoryEntry[], session: string): string[] {
  const newest = [...list].reverse();
  const own = newest.filter((e) => e.session === session);
  const rest = newest.filter((e) => e.session !== session);
  return [...new Set([...own, ...rest].map((e) => e.text))];
}

/** The draft from before history browsing, and the shown entry. */
export type Browse = { index: number; draft: string };

/** Up or Down in the box. Null leaves the key to the text area.
 *  Up starts browsing only in an empty box or with the caret at 0, so a first-line Up still moves
 *  the caret home. Browsing goes on while the box shows the entry unchanged: Up from the first
 *  line goes older, Down from the last line goes newer, and Down past the newest gives the draft
 *  back. */
export function historyStep(
  entries: readonly string[],
  browse: Browse | null,
  text: string,
  caret: number,
  dir: "up" | "down",
): { text: string; caret: number; browse: Browse | null } | null {
  const shown = browse !== null && entries[browse.index] === text ? browse : null;
  if (dir === "up") {
    if (shown) {
      if (text.lastIndexOf("\n", caret - 1) >= 0 || shown.index + 1 >= entries.length) return null;
      return { text: entries[shown.index + 1], caret: 0, browse: { ...shown, index: shown.index + 1 } };
    }
    if ((text !== "" && caret !== 0) || !entries.length) return null;
    return { text: entries[0], caret: 0, browse: { index: 0, draft: text } };
  }
  if (!shown || text.indexOf("\n", caret) >= 0) return null;
  const next = shown.index === 0 ? shown.draft : entries[shown.index - 1];
  return { text: next, caret: next.length, browse: shown.index === 0 ? null : { ...shown, index: shown.index - 1 } };
}
