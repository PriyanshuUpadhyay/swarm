// Command registry. One global keydown listener and the ⌘K palette both read it.

export type Command = {
  id: string;
  title: string;
  /** Modifiers and one key joined by "+", e.g. "Ctrl+Meta+ArrowDown", "Shift+Meta+]", "j". */
  chord?: string;
  /** Hidden from the palette when false. */
  palette?: boolean;
  /** The phase that brings the command's target; until then it only says so. */
  phase?: string;
  /** Extra gate for keys without ⌘ or ⌃, such as the tree keys. */
  when?: () => boolean;
  run: (e?: KeyboardEvent) => void;
};

declare global {
  interface Window {
    /** The native shell's menu items call this. False when no command with that id ran. */
    swarmCommand?: (id: string) => boolean;
  }
}

type Chord = { ctrl: boolean; meta: boolean; shift: boolean; alt: boolean; key: string };

export function parseChord(chord: string): Chord {
  const parts = chord.split("+");
  // "Shift+Meta++" would name the plus key; this pilot has none.
  const key = parts.pop()!.toLowerCase();
  const mods = new Set(parts.map((p) => p.toLowerCase()));
  return { ctrl: mods.has("ctrl"), meta: mods.has("meta"), shift: mods.has("shift"), alt: mods.has("alt"), key };
}

type KeyLike = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

/** The physical key for letters, digits and brackets, so Shift and Option do not change it. */
export function eventKey(e: KeyLike): string {
  const c = e.code;
  if (c.startsWith("Key")) return c.slice(3).toLowerCase();
  if (c.startsWith("Digit")) return c.slice(5);
  if (c === "BracketRight") return "]";
  if (c === "BracketLeft") return "[";
  return e.key.toLowerCase();
}

export function matches(chord: Chord, e: KeyLike): boolean {
  return (
    chord.ctrl === e.ctrlKey &&
    chord.meta === e.metaKey &&
    chord.shift === e.shiftKey &&
    chord.alt === e.altKey &&
    chord.key === eventKey(e)
  );
}

export class Registry {
  private cmds: { cmd: Command; chord: Chord | null }[] = [];

  add(...cmds: Command[]) {
    for (const cmd of cmds) this.cmds.push({ cmd, chord: cmd.chord ? parseChord(cmd.chord) : null });
  }

  list(): Command[] {
    return this.cmds.map((c) => c.cmd);
  }

  match(e: KeyLike): Command | undefined {
    for (const { cmd, chord } of this.cmds) if (chord && matches(chord, e) && (!cmd.when || cmd.when())) return cmd;
    return undefined;
  }

  /** Runs the command with this id, without a key event. */
  run(id: string): boolean {
    const cmd = this.cmds.find((c) => c.cmd.id === id)?.cmd;
    if (!cmd || (cmd.when && !cmd.when())) return false;
    cmd.run();
    return true;
  }
}

function editable(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  return t.isContentEditable || t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT";
}

/** Installs the one global listener and `swarmCommand`. Keys without ⌘ or ⌃ never fire inside a text input. */
export function installKeys(registry: Registry, target: Window = window): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.isComposing || (!e.metaKey && !e.ctrlKey && editable(e.target))) return;
    const cmd = registry.match(e);
    if (!cmd) return;
    e.preventDefault();
    cmd.run(e);
  };
  target.addEventListener("keydown", onKey);
  target.swarmCommand = (id) => registry.run(id);
  return () => {
    target.removeEventListener("keydown", onKey);
    delete target.swarmCommand;
  };
}
