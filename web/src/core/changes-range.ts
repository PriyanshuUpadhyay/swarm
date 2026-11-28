import type { CommentEditor, FileDiff } from "./changes-types";

export function sameRange(preview: FileDiff, full: FileDiff, editor: CommentEditor | null) {
  if (!editor) return true;
  const before = preview.display?.lines ?? [];
  const after = full.display?.lines ?? [];
  const from = Math.min(editor.anchor, editor.cursor);
  const to = Math.max(editor.anchor, editor.cursor);

  return before.length > to && after.length > to && before.slice(from, to + 1).every((line, i) => {
    const same = after[from + i];
    if (JSON.stringify(line) === JSON.stringify(same)) return true;
    if (!preview.partial_last_line || from + i !== before.length - 1) return false;
    const { text, quote, ...spot } = line;
    const { text: fullText, quote: fullQuote, ...fullSpot } = same;
    return JSON.stringify(spot) === JSON.stringify(fullSpot) && fullText.startsWith(text)
      && (quote === undefined ? fullQuote === undefined : fullQuote?.startsWith(quote));
  });
}
