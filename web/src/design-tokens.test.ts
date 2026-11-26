// DESIGN.md owns the colours; styles-base.css must carry the same values in dark and in both light blocks.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const design = read("../../DESIGN.md");
const css = read("./styles-base.css");

// One design role can feed more than one CSS variable.
const roleToVariables: Record<string, string[]> = {
  surface: ["--background", "--sidebar"],
  panel: ["--surface", "--code"],
  selected: ["--selected"], // `--accent` is `--color-accent: var(--selected)` in `@theme inline`
  "row-hover": ["--hover"],
  border: ["--border"],
  "on-surface": ["--foreground", "--ring"],
  muted: ["--muted-foreground"],
  "disabled-text": ["--disabled-foreground"],
  "state-failed": ["--status-failed"],
  "state-waiting": ["--status-waiting"],
  "state-done": ["--status-done"],
  "state-working": ["--status-working"],
  "state-ended": ["--status-ended"],
  "glyph-lit": ["--glyph-lit"],
  "glyph-unlit": ["--glyph-unlit"],
  "pr-quiet-text": ["--pr-quiet"],
  "pr-draft-border": ["--pr-draft-border"],
  menu: ["--popover"],
  "menu-border": ["--menu-border"],
  "menu-hover": ["--menu-hover"],
  "menu-selected": ["--menu-selected"],
  tooltip: ["--tooltip"],
  "primary-hover": ["--brand-hover"],
  "primary-pressed": ["--brand-pressed"],
  "primary-disabled": ["--brand-disabled"],
  "quiet-hover": ["--quiet-hover"],
  "quiet-pressed": ["--quiet-pressed"],
  "shadow-1": ["--shadow-1"],
  "shadow-2": ["--shadow-2"],
};
// `primary` and `on-primary` have one value for both themes.
const sharedRoleToVariable: Record<string, string> = { primary: "--brand", "on-primary": "--brand-foreground" };

type Theme = "dark" | "light";
const quoted = (value: string) => value.replace(/\s+#.*$/, "").trim().replace(/^"(.*)"$/, "$1");

// The front matter is a small, regular subset of YAML, so no parser package is needed.
function frontMatterTokens(): { shared: Record<string, string>; themes: Record<Theme, Record<string, string>> } {
  const front = design.split(/^---$/m)[1] ?? "";
  const shared: Record<string, string> = {};
  const themes: Record<Theme, Record<string, string>> = { dark: {}, light: {} };
  let top = "";
  let theme: Theme | "" = "";

  for (const line of front.split("\n")) {
    const top1 = /^(\w[\w-]*):/.exec(line);
    if (top1) {
      top = top1[1];
      theme = "";
      continue;
    }
    const level2 = /^ {2}([\w-]+):(.*)$/.exec(line);
    if (level2 && (top === "colors" || top === "shadow")) {
      const [, key, rest] = level2;
      if (key === "dark" || key === "light") theme = key;
      else if (top === "colors") shared[key] = quoted(rest);
      continue;
    }
    const level3 = /^ {4}([\w-]+):\s*(.+)$/.exec(line);
    if (level3 && theme && (top === "colors" || top === "shadow")) themes[theme][level3[1]] = quoted(level3[2]);
  }

  return { shared, themes };
}

function block(opening: RegExp, closing: string): Record<string, string> {
  const start = opening.exec(css);
  if (!start) throw new Error(`no block for ${opening}`);
  const end = css.indexOf(closing, start.index);
  if (end < 0) throw new Error(`no end for ${opening}`);
  const text = css.slice(start.index, end);
  const out: Record<string, string> = {};
  for (const m of text.matchAll(/^\s*(--[\w-]+):\s*([^;]+);/gm)) out[m[1]] = m[2].trim();
  return out;
}

const dark = block(/^:root,\n:root\[data-theme="dark"\] \{/m, "\n}\n");
const lightMedia = block(/^ {2}:root:not\(\[data-theme\]\) \{/m, "\n  }\n");
const lightAttr = block(/^:root\[data-theme="light"\] \{/m, "\n}\n");
const tokens = frontMatterTokens();

describe("design tokens", () => {
  it.each(["dark", "light"] as const)("%s CSS variables carry the DESIGN.md colours", (theme) => {
    const actual = theme === "dark" ? dark : lightAttr;
    const expected: Record<string, string> = {};
    for (const [role, variables] of Object.entries(roleToVariables)) {
      const value = tokens.themes[theme][role];
      expect(value, `${role} is missing from the ${theme} front matter`).toBeDefined();
      for (const variable of variables) expected[variable] = value;
    }
    for (const [role, variable] of Object.entries(sharedRoleToVariable)) expected[variable] = tokens.shared[role];
    const found = Object.fromEntries(Object.keys(expected).map((variable) => [variable, actual[variable]]));
    expect(found).toEqual(expected);
  });

  it("the two light blocks declare the same variables with the same values", () => {
    expect(lightMedia).toEqual(lightAttr);
  });

  it("the app shells use the DESIGN.md dark surface colour", () => {
    const surface = tokens.themes.dark.surface;
    expect(surface).toBeDefined();

    const html = read("../index.html");
    expect(html.match(/<meta name="theme-color" content="([^"]+)"/)?.[1], "HTML theme-color").toBe(surface);

    const manifest = JSON.parse(read("../public/manifest.webmanifest"));
    expect(manifest.background_color, "manifest background_color").toBe(surface);
    expect(manifest.theme_color, "manifest theme_color").toBe(surface);

    const swift = read("../../ui/Sources/SwarmApp/main.swift");
    const windowRgb = swift.match(/window\.backgroundColor = NSColor\(red: 0x([\da-f]{2}) \/ 255, green: 0x([\da-f]{2}) \/ 255, blue: 0x([\da-f]{2}) \/ 255,/i);
    expect(windowRgb && `#${windowRgb.slice(1).join("").toLowerCase()}`, "Swift window background").toBe(surface);
  });
});
