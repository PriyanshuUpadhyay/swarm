// System, light or dark. Dark is the CSS default; "system" follows prefers-color-scheme. The owner's
// choice is `prefs.theme` in choices.json (S5); this browser keeps a copy so the first paint is right.

import { useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { store } from "./store";

export type Theme = "system" | "light" | "dark";
const KEY = "swarm.theme";

const read = (): Theme => {
  const v = store.getItem(KEY);
  return v === "light" || v === "dark" ? v : "system";
};

let current: Theme = read();
const listeners = new Set<() => void>();

export function applyTheme() {
  const root = document.documentElement;
  if (current === "system") delete root.dataset.theme;
  else root.dataset.theme = current;
}

export function setTheme(theme: Theme) {
  current = theme;
  store.setItem(KEY, theme);
  applyTheme();
  for (const fn of listeners) fn();
}

/** Shows the theme now and saves it as the owner's choice; a refused write puts the old one back. */
export function chooseTheme(app: App, theme: Theme) {
  const before = current;
  setTheme(theme);
  void app.settings.setPref("theme", theme).then((ok) => ok || setTheme(before));
}

/** Follows `prefs.theme` when another window or the Swift app changes it. Only a changed value
 *  applies, so a tree read that started before a local choice does not undo it. */
export function followTheme(app: App) {
  let seen: string | undefined;
  const off = app.subscribe(() => {
    const theme = app.view.tree?.prefs?.theme;
    if (theme === seen) return;
    seen = theme;
    if ((theme === "system" || theme === "light" || theme === "dark") && theme !== current) setTheme(theme);
  });
  return () => void off();
}

const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => listeners.delete(fn);
};

export const useTheme = () => useSyncExternalStore(subscribe, () => current);
