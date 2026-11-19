// Syntax colours for Markdown code blocks, from @pierre/highlights. Load it through code-tokens.tsx.
import { codeToTokens, isSupportedLanguage } from "@pierre/highlights";
import dark from "@pierre/highlights/themes/pierre-dark";
import light from "@pierre/highlights/themes/pierre-light";
import type { CSSProperties, ReactNode } from "react";

// A longer line stays one plain run, as in Shiki, so a minified file does not make thousands of spans.
const MAX_LINE = 2000;

function style(css: Record<string, string> | undefined, color: string | undefined): CSSProperties | undefined {
  if (!css) return color ? { color } : undefined;
  return Object.fromEntries(Object.entries(css).map(([k, v]) => [k.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase()), v]));
}

/** The code as coloured runs, or null for an unknown language. The runs cover every UTF-16 unit
 *  of `text`, so copy and find read the same text as plain code. */
export function codeRuns(text: string, lang: string): ReactNode[] | null {
  const key = lang.toLowerCase();
  if (!isSupportedLanguage(key)) return null;
  // `light-dark()` follows the page's color-scheme, so one pass serves both themes.
  const { tokens } = codeToTokens(text, { lang: key, themes: { light, dark }, defaultColor: "light-dark()", tokenizeMaxLineLength: MAX_LINE });
  const out: ReactNode[] = [];
  let at = 0;
  for (const token of tokens.flat()) {
    if (token.offset > at) out.push(text.slice(at, token.offset));
    const end = token.offset + token.content.length;
    out.push(<span key={token.offset} style={style(token.htmlStyle, token.color)}>{text.slice(token.offset, end)}</span>);
    at = end;
  }
  if (at < text.length) out.push(text.slice(at));
  return out;
}
