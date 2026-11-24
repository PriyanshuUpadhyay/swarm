/** Text that Find never matches. Note cards and the Whole reply handle sit in a row (ADR 0161),
 *  but the server, which counts the matches, does not see them. */
const SKIP = '[aria-hidden="true"],.ln,[data-gutter],script,style,.ncard,[data-note-reply]';

/** The text nodes under `root` in reading order, inside open shadow roots too, such as a Pierre patch. */
function textNodes(root: Node, nodes: Text[]) {
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
  for (let node: Node | null = walk.currentNode; node; node = walk.nextNode()) {
    if (node instanceof Text) {
      if (!node.parentElement?.closest(SKIP)) nodes.push(node);
    } else if (node instanceof Element && node.shadowRoot) textNodes(node.shadowRoot, nodes);
  }
  return nodes;
}

/** Text ranges can cross markdown spans without changing React's nodes. A range stays inside one
 *  shadow root, because a match is never longer than one line. */
export function textRanges(root: Node, query: string): Range[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const nodes = textNodes(root, []);
  const text = nodes.map((n) => n.data).join("").toLowerCase();
  const ranges: Range[] = [];
  for (let at = text.indexOf(q); at >= 0; at = text.indexOf(q, at + q.length)) {
    const range = document.createRange();
    let offset = 0;
    let started = false;
    for (const node of nodes) {
      if (!started && at < offset + node.length) { range.setStart(node, at - offset); started = true; }
      if (started && at + q.length <= offset + node.length) { range.setEnd(node, at + q.length - offset); break; }
      offset += node.length;
    }
    ranges.push(range);
  }
  return ranges;
}

export function paintRanges(name: string, ranges: Range[]) {
  if (typeof Highlight !== "undefined" && CSS.highlights) CSS.highlights.set(name, new Highlight(...ranges));
}
