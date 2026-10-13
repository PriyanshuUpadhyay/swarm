// System, light or dark. Dark is the CSS default; "system" follows prefers-color-scheme.

import { useSyncExternalStore } from "react";
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

const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => listeners.delete(fn);
};

export const useTheme = () => useSyncExternalStore(subscribe, () => current);
