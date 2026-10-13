// Turns a registry chord such as "Ctrl+Meta+ArrowDown" into the Mac label "⌃⌘↓".

const GLYPH: Record<string, string> = {
  ctrl: "⌃",
  alt: "⌥",
  shift: "⇧",
  meta: "⌘",
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  enter: "↩",
  escape: "⎋",
  backspace: "⌫",
  tab: "⇥",
};

// Mac menus list modifiers in this order, whatever order the chord names them in.
const ORDER = ["ctrl", "alt", "shift", "meta"];

export function chordLabel(chord: string): string {
  const parts = chord.split("+");
  const key = parts.pop()!;
  const mods = parts.map((p) => p.toLowerCase()).sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return mods.map((m) => GLYPH[m] ?? m).join("") + (GLYPH[key.toLowerCase()] ?? key.toUpperCase());
}
