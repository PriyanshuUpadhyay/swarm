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

/** A menu pick names one occurrence, so a typed copy is still plain text. */
export type SkillPick = { start: number; insert: string };

export function validPicks(text: string, picks: readonly SkillPick[]): SkillPick[] {
  return picks.filter((p) => p && Number.isInteger(p.start) && p.start >= 0 && typeof p.insert === "string"
    && /^\/skill:\S+$/.test(p.insert) && text.slice(p.start, p.start + p.insert.length) === p.insert
    && (p.start === 0 || /[\s([]/.test(text[p.start - 1]))
    && (p.start + p.insert.length === text.length || /\s/.test(text[p.start + p.insert.length])))
    .sort((a, b) => a.start - b.start).filter((p, i, all) => !i || p.start !== all[i - 1].start);
}

/** Move the saved ranges with a native text edit; an edit inside a token removes its pick. */
export function editPicks(before: string, after: string, picks: readonly SkillPick[], edit?: { start: number; end: number }): SkillPick[] {
  if (!picks.length) return [];
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  let end = before.length;
  let nextEnd = after.length;
  while (end > start && nextEnd > start && before[end - 1] === after[nextEnd - 1]) { end--; nextEnd--; }
  // Native edit ranges distinguish two equal tokens when one occurrence is deleted.
  if (edit) { start = edit.start; end = edit.end; nextEnd = end + after.length - before.length; }
  return validPicks(after, picks.flatMap((p) => {
    if (p.start + p.insert.length <= start) return [p];
    if (p.start >= end) return [{ ...p, start: p.start + nextEnd - end }];
    return [];
  }));
}

/** The token at the caret that opened the menu. `end` reaches past the caret to the token's end,
 *  so a pick replaces the whole word. */
export type Menu = { opener: "/" | "$"; start: number; end: number; query: string };

/** The menu for the caret, or null. A `/` at the draft's start offers every command. Mid-draft,
 *  Claude and AGY run only skills and commands, pi only skills, and Codex none. A `$` is a Codex skill. */
export function menuAt(draft: string, caret: number, provider: string | null): Menu | null {
  const slash = token(draft, caret, "/");
  if (slash) return slash.start === 0 || provider !== "codex" ? slash : null;
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

/** The rows this menu can offer: built-ins only at the draft's start. pi expands only skills
 *  mid-draft, and only after `skillFirst` moves them to the start. */
export function offered(rows: readonly MenuRow[], menu: Menu, provider: string | null = null): MenuRow[] {
  const midDraft = (r: MenuRow) => (provider === "pi" ? r.kind === "skill" : r.kind !== "builtin");
  return rows.filter((r) => r.insert.startsWith(menu.opener) && (atStart(menu) || midDraft(r)));
}

// Normalize the token's separator to a space before pi reads its skill name.
const SKILL = /(^|[\s([])(\/skill:\S+)/g;

/** True on a `>` quote line or inside a ``` fence, where the owner shows a skill and does not run it. */
function quoted(text: string, at: number): boolean {
  const line = text.slice(text.lastIndexOf("\n", at - 1) + 1, at);
  return /^\s*>/.test(line) || (text.slice(0, at).match(/^\s*```/gm)?.length ?? 0) % 2 === 1;
}

/** pi expands one skill at the message's start. The first recorded pick moves there; without
 *  picks, use the first typed token in pi's list. `extra` holds the tokens sent as text. */
export function skillFirst(text: string, skills: ReadonlySet<string>, picks: readonly SkillPick[] = []): { text: string; extra: string[] } {
  const recorded = validPicks(text, picks);
  const found = [...text.matchAll(SKILL)]
    .filter((m) => skills.has(m[2]) && !quoted(text, m.index + m[1].length))
    .map((m) => ({ start: m.index + m[1].length, insert: m[2] }));
  const first = recorded[0] ?? found[0];
  const extra = validPicks(text, [...recorded, ...found]).filter((p) => p.start !== first?.start).map((p) => p.insert);
  if (!first) return { text, extra };
  const words = (text.slice(0, first.start) + text.slice(first.start + first.insert.length).replace(/^\s+/, "")).trim();
  return { text: words ? `${first.insert} ${words}` : first.insert, extra };
}

/** The `/skill:<name>` inserts of pi's skill rows. */
export const skillNames = (list: CommandList | null) => new Set(list?.commands.filter((r) => r.kind === "skill").map((r) => r.insert));

/** Recorded picks need no probe. Only typed tokens use pi's list, loading it when absent. */
export async function toSend(text: string, provider: string | null, list: CommandList | null, load: () => Promise<CommandList>, picks: readonly SkillPick[] = []) {
  if (provider !== "pi") return { text, list };
  if (validPicks(text, picks).length) return { text: skillFirst(text, skillNames(list), picks).text, list };
  const used = list ?? (text.includes("/skill:") ? await load().catch(() => null) : null);
  return { text: skillFirst(text, skillNames(used)).text, list: used };
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
