/** Text ranges can cross markdown spans without changing React's nodes. */
export function textRanges(root: HTMLElement, query: string): Range[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const nodes: Text[] = [];
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  while (walk.nextNode()) {
    const node = walk.currentNode as Text;
    if (!node.parentElement?.closest('[aria-hidden="true"],.ln,script,style')) nodes.push(node);
  }
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
