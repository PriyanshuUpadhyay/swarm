// The composer's `/` and `$` menu, ported from the SwiftUI app's ComposerMenu and its key router
// (`58beca2a^:ui/Sources/SwarmCore/Composer/ComposerMenu.swift`). Pure, so vitest covers it.

import { fuzzySearchItems } from "./fuzzy-search";

/** One `swarm commands` row. `insert` is the exact text a pick puts in the draft. */
export type MenuRow = {
  name: string;
  insert: string;
  kind: "builtin" | "command" | "skill";
  description: string;
  hint: string;
  source: string;
  /** Opens a picker in the CLI's pane, so the menu inserts it and never sends it. */
  dialog: boolean;
};

export type CommandList = { commands: MenuRow[]; note: string | null };

/** The token at the caret that opened the menu. `end` reaches past the caret to the token's end,
 *  so a pick replaces the whole word. */
export type Menu = { opener: "/" | "$"; start: number; end: number; query: string };

/** The menu for the caret, or null. A `/` at the draft's start offers every command. Mid-draft,
 *  Claude and AGY run only skills and commands, while Codex and pi run none. A `$` is a Codex skill. */
export function menuAt(draft: string, caret: number, provider: string | null): Menu | null {
  const slash = token(draft, caret, "/");
  if (slash) return slash.start === 0 || (provider !== "codex" && provider !== "pi") ? slash : null;
  return provider === "codex" ? token(draft, caret, "$") : null;
}

function token(draft: string, caret: number, opener: "/" | "$"): Menu | null {
  const at = draft.lastIndexOf(opener, caret - 1);
  if (at < 0) return null;
  const query = draft.slice(at + 1, caret);
  if (/\s/.test(query)) return null;
  if (at > 0 && !" \n\t([".includes(draft[at - 1])) return null;
  const rest = draft.slice(caret).search(/\s/);
  return { opener, start: at, end: rest < 0 ? draft.length : caret + rest, query };
}

const atStart = (menu: Menu) => menu.opener === "/" && menu.start === 0;

/** The rows this menu can offer: built-ins only at the draft's start. */
export function offered(rows: readonly MenuRow[], menu: Menu): MenuRow[] {
  return rows.filter((r) => r.insert.startsWith(menu.opener) && (atStart(menu) || r.kind !== "builtin"));
}

/** Mid-draft a `/` or `$` can be a path or a price, so that menu shows only with matching rows. */
export const showsEmpty = atStart;

/** The rows that match the query, best first: the exact name, then names that start with it,
 *  then jean's fuzzy order. */
export function matches(rows: readonly MenuRow[], query: string, limit = 12): MenuRow[] {
  const q = query.toLowerCase();
  const rank = (r: MenuRow) => (r.name.toLowerCase() === q ? 0 : r.name.toLowerCase().startsWith(q) ? 1 : 2);
  return fuzzySearchItems([...rows], query, 50)
    .map((r, i) => ({ r, i }))
    .sort((a, b) => rank(a.r) - rank(b.r) || a.i - b.i)
    .slice(0, limit)
    .map(({ r }) => r);
}

/** The draft after a pick, and where the caret goes. A space follows unless one is already there. */
export function pick(draft: string, menu: Menu, row: MenuRow): { text: string; caret: number } {
  const after = draft.slice(menu.end);
  const value = row.insert + (/^\s/.test(after) ? "" : " ");
  return { text: draft.slice(0, menu.start) + value + after, caret: menu.start + value.length };
}

/** The send key on a command that is the whole draft runs it, as the CLI does on Enter. A dialog
 *  row would leave a picker open in a pane the owner cannot see, so it is only inserted. */
export function picksAndSends(draft: string, menu: Menu, row: MenuRow): boolean {
  return atStart(menu) && menu.end === draft.length && !row.dialog;
}

/** Up and Down wrap around the rows. */
export function moved(current: number, count: number, delta: number): number {
  return count > 0 ? (current + (delta % count) + count) % count : 0;
}
