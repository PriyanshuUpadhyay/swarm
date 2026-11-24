// The state of a text box's `/`, `$` and `@` menu, shared by the chat's box and the new chat
// composer (ADR 0155). Each box renders `SlashMenu` with it and inserts the picked row itself.

import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { matches, menuAt, moved, offered, showsEmpty, type CommandList, type MenuRow } from "../core/slash";

/** `source` names where the lists come from, such as the chat or the folder and its CLI; a new
 *  source drops the old lists. `commands` and `files` read them. */
export function useSlashMenu({ text, provider, disabled, source, commands, files }: {
  text: string;
  provider: string | null;
  disabled: boolean;
  source: string;
  commands: () => Promise<CommandList>;
  files: () => Promise<CommandList>;
}) {
  const [caret, setCaret] = useState(0);
  const [focused, setFocused] = useState(false);
  const [list, setList] = useState<CommandList | null>(null);
  const [fileList, setFileList] = useState<CommandList | null>(null);
  const [selected, setSelected] = useState(0);
  // The start of the token whose menu Escape closed; the menu stays shut until the token changes.
  const [dismissed, setDismissed] = useState<number | null>(null);
  const listId = useId();
  // The readers of this render, for the effect that runs only when the menu or the source changes.
  const read = useRef({ commands, files });
  useLayoutEffect(() => {
    read.current = { commands, files };
  });

  const menu = focused && !disabled ? menuAt(text, caret, provider) : null;
  const shownList = menu?.opener === "@" ? fileList : list;
  const rows = menu && shownList ? matches(offered(shownList.commands, menu, provider), menu.query) : [];
  const open = menu !== null && dismissed !== menu.start && (rows.length > 0 || showsEmpty(menu, shownList));
  const current = Math.min(selected, Math.max(rows.length - 1, 0));
  const menuKey = menu ? `${menu.opener}${menu.start}` : null;

  const lastSource = useRef(source);
  useEffect(() => {
    if (lastSource.current === source) return;
    lastSource.current = source;
    setList(null);
    setFileList(null);
  }, [source]);
  // A focus with no list starts the CLI's slow first read (about 1 s for pi), so the first `/`
  // finds it ready or under way.
  useEffect(() => {
    if (!focused || disabled || list) return;
    let live = true;
    read.current.commands().then(
      (next) => live && setList((old) => old ?? next),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [focused, disabled, source, list === null]);
  // Each opening asks again; the server answers from its cache and refreshes a stale list.
  useEffect(() => {
    if (!menuKey) {
      setDismissed(null);
      return;
    }
    let live = true;
    // The file list stays apart, so pi's skill list keeps its rows for the send.
    const set = menuKey.startsWith("@") ? setFileList : setList;
    (menuKey.startsWith("@") ? read.current.files() : read.current.commands()).then(
      (next) => live && set(next),
      (err: unknown) => live && set((old) => ({ commands: old?.commands ?? [], note: err instanceof Error ? err.message : String(err) })),
    );
    return () => {
      live = false;
    };
  }, [source, menuKey]);
  useEffect(() => {
    setSelected(0);
  }, [menuKey, menu?.query]);
  useEffect(() => {
    document.getElementById(`${listId}-${current}`)?.scrollIntoView({ block: "nearest" });
  }, [listId, current]);

  /** The open menu's own keys: ↑ ↓ move, Esc closes it, and Tab inserts the row. True when the
   *  menu took the key. Enter stays with the box, because each box sends on another key. */
  const onKeyDown = (e: KeyboardEvent, insert: (row: MenuRow) => void): boolean => {
    if (!open || !menu) return false;
    const row = rows[current];
    if (e.key === "ArrowDown" || e.key === "ArrowUp") setSelected(moved(current, rows.length, e.key === "ArrowDown" ? 1 : -1));
    else if (e.key === "Escape") setDismissed(menu.start);
    else if (row && e.key === "Tab" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) insert(row);
    else return false;
    e.preventDefault();
    return true;
  };

  return { menu, rows, open, current, row: open ? rows[current] : undefined, listId, list, setList, shownList, setCaret, setFocused, setSelected, setDismissed, onKeyDown };
}
