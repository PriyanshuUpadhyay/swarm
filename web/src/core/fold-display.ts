// A long reply shown shorter, from its own sentences: no model call and nothing rewritten.
// Points keeps each paragraph's and each list item's first sentence, and Outline keeps the headings
// with the first sentence under each. The sentence rule and the thresholds follow TextFold's
// fold.ts (Apache-2.0, https://github.com/chenxiachan/textfold, plugins/textfold/hooks/fold.ts).
import type { Block, Display, Inline } from "./api";
import { cut, units } from "./transcript";

export type FoldLevel = "full" | "points" | "outline";

/** Below this many words a reply is read whole. */
const MIN_WORDS = 80;
/** A fold must hide at least this share of the words, or the reader would see no change. */
const MIN_HIDDEN = 0.25;
/** A longer code block folds to one line that counts its lines. */
const CODE_LINES = 6;

const MORE: Inline = { type: "text", value: " …" };
const ABBREV = /\b(?:e\.g|i\.e|etc|vs|cf|al|approx|Dr|Mr|Ms|Fig|Eq|No)\.$/i;

/** The inline text, with a mark on each unit that sits in inline code, where no sentence ends. */
function plain(nodes: readonly Inline[]): { text: string; code: boolean[] } {
  let text = "";
  const code: boolean[] = [];
  const add = (value: string, inCode: boolean) => {
    text += value;
    for (let i = 0; i < value.length; i++) code.push(inCode);
  };
  const walk = (list: readonly Inline[]) => {
    for (const node of list) {
      switch (node.type) {
        case "text": add(node.value, false); break;
        case "code": add(node.value, true); break;
        case "break": add("\n", false); break;
        case "emphasis":
        case "strong":
        case "delete": walk(node.value); break;
        case "link": walk(node.value.children); break;
        case "later": break;
      }
    }
  };
  walk(nodes);
  return { text, code };
}

/** Where the first sentence ends, or null when the text is one sentence. "3.5" and "e.g." do not end one. */
function sentenceEnd(nodes: readonly Inline[]): number | null {
  const { text, code } = plain(nodes);
  for (let i = 0; i < text.length; i++) {
    if (code[i]) continue;
    const c = text[i] ?? "";
    if ("。！？".includes(c)) return i + 1 < text.trim().length ? i + 1 : null;
    if (!".!?".includes(c)) continue;
    const after = /^([)"'”’\]]*)(\s+|$)/.exec(text.slice(i + 1));
    if (!after || ABBREV.test(text.slice(0, i + 1))) continue;
    const end = i + 1 + (after[1] ?? "").length;
    return text.slice(end).trim() ? end : null;
  }
  return null;
}

/** A paragraph's inlines cut to its first sentence, with a mark when more follows. */
function firstSentence(nodes: Inline[]): Inline[] {
  const end = sentenceEnd(nodes);
  if (end === null) return nodes;
  const [para] = cut({ blocks: [{ type: "paragraph", value: nodes }] }, end).blocks;
  return para?.type === "paragraph" ? [...para.value, MORE] : nodes;
}

/** A block's first sentence as a paragraph, for an item, a quote, or the line under a heading. */
function lead(block: Block | undefined): Block | null {
  switch (block?.type) {
    case "paragraph": return { type: "paragraph", value: firstSentence(block.value) };
    case "blockquote": {
      const inner = lead(block.value[0]);
      return inner ? { type: "blockquote", value: [inner] } : null;
    }
    case "list": return lead(block.value.items[0]?.[0]);
    default: return null;
  }
}

function points(blocks: Block[]): Block[] {
  const out: Block[] = [];
  for (const block of blocks) {
    switch (block.type) {
      case "paragraph":
      case "blockquote": {
        const kept = lead(block);
        if (kept) out.push(kept);
        break;
      }
      case "list": {
        // Each item keeps its first sentence, and its nested lists go.
        const items = block.value.items.map((item) => {
          const kept = lead(item[0]);
          return kept ? [kept] : [];
        });
        out.push({ type: "list", value: { ...block.value, tight: true, items } });
        break;
      }
      case "code": {
        // A short block is often the command the reply is about, so it stays. Tables stay whole
        // under default, because a table is often the answer.
        const lines = block.value.text.split("\n").length;
        out.push(lines <= CODE_LINES ? block : { type: "paragraph", value: [{ type: "code", value: `⋯ ${lines} lines of code` }] });
        break;
      }
      case "thematic_break":
        break;
      default:
        out.push(block);
    }
  }
  return out;
}

function outline(blocks: Block[]): Block[] {
  if (!blocks.some((b) => b.type === "heading")) return points(blocks);
  const out: Block[] = [];
  let underHeading = true;
  for (const block of blocks) {
    if (block.type === "heading") {
      out.push(block);
      underHeading = true;
      continue;
    }
    if (!underHeading) continue;
    const kept = lead(block);
    if (kept) {
      out.push(kept);
      underHeading = false;
    }
  }
  return out;
}

function words(d: Display): number {
  return units(d).match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu)?.length ?? 0;
}

/** The reply at a level, or null when that level would not shorten it enough to help. */
export function fold(d: Display, level: FoldLevel): Display | null {
  if (level === "full") return d;
  const all = words(d);
  if (all < MIN_WORDS) return null;
  const folded = { blocks: level === "points" ? points(d.blocks) : outline(d.blocks) };
  return 1 - words(folded) / all >= MIN_HIDDEN ? folded : null;
}
