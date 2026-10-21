// The owner's appearance choices: the theme, the UI font and the code font. Each is a `prefs` key in
// choices.json (S5) and an attribute on <html> (`data-theme`, `data-font-sans`, `data-font-mono`);
// "system" removes the attribute, so the CSS default applies. Dark is the CSS default theme;
// "system" follows prefers-color-scheme. This browser keeps a copy so the first paint is right.
// Zoom is not here: Swarm.app owns it (View › Zoom In), and a browser tab uses the browser's zoom.

import { useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { store } from "./store";

type Key = "theme" | "fontSans" | "fontMono";

function ownerChoice<T extends string>(key: Key, values: readonly T[]) {
  const storeKey = `swarm.${key}`;
  const known = (v: unknown): v is T => values.includes(v as T);
  const saved = store.getItem(storeKey);
  let current = (known(saved) ? saved : "system") as T;
  const listeners = new Set<() => void>();

  const apply = () => {
    const root = document.documentElement;
    if (current === "system") delete root.dataset[key];
    else root.dataset[key] = current;
  };
  const set = (value: T) => {
    current = value;
    store.setItem(storeKey, value);
    apply();
    for (const fn of listeners) fn();
  };
  const subscribe = (fn: () => void) => {
    listeners.add(fn);
    return () => listeners.delete(fn);
  };

  return {
    apply,
    /** Shows the choice now and saves it as the owner's; a refused write puts the old one back. */
    choose(app: App, value: T) {
      const before = current;
      set(value);
      void app.settings.setPref(key, value).then((ok) => ok || set(before));
    },
    /** Follows `prefs[key]` when another window changes it. Only a changed value applies, so a tree
     *  read that started before a local choice does not undo it. An absent or unknown value is
     *  "system", so this browser's copy never outlives the owner's record. */
    follow(app: App) {
      let seen: T | undefined;
      const off = app.subscribe(() => {
        const tree = app.view.tree;
        if (!tree) return;
        const saved = tree.prefs?.[key];
        const value = known(saved) ? saved : ("system" as T);
        if (value === seen) return;
        seen = value;
        if (value !== current) set(value);
      });
      return () => void off();
    },
    useChoice: () => useSyncExternalStore(subscribe, () => current),
  };
}

export type Theme = "system" | "light" | "dark";
export type FontSans = "system" | "helvetica" | "avenir";
export type FontMono = "system" | "menlo" | "monaco";

const theme = ownerChoice<Theme>("theme", ["system", "light", "dark"]);
export const fontSans = ownerChoice<FontSans>("fontSans", ["system", "helvetica", "avenir"]);
export const fontMono = ownerChoice<FontMono>("fontMono", ["system", "menlo", "monaco"]);
const ALL = [theme, fontSans, fontMono];

export const chooseTheme = theme.choose;
export const useTheme = theme.useChoice;

export function applyAppearance() {
  for (const c of ALL) c.apply();
}

export function followAppearance(app: App) {
  const offs = ALL.map((c) => c.follow(app));
  return () => offs.forEach((off) => off());
}
