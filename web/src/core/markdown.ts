// Diff fence HTML for the typed display painter.

import { html as diffHtml } from "diff2html";

const esc = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** A diff fence with hunks, rendered by diff2html in both layouts. CSS shows the layout that the
 *  `data-diff-split` attribute on <html> picks. Agents often write hunks with wrong counts or with
 *  no `---`/`+++` header. diff2html still reads wrong counts, but it drops a hunk that has no file
 *  header, so a bare fence gets a placeholder header, which CSS hides. */
export function diffFence(patch: string): { headed: boolean; html: string } {
  const headed = /^--- /m.test(patch.slice(0, patch.search(/^@@ /m)));
  const src = headed ? patch : `--- a/patch\n+++ b/patch\n${patch}`;
  const layout = (outputFormat: "line-by-line" | "side-by-side") => diffHtml(src, { drawFileList: false, outputFormat });
  return {
    headed,
    html: `<div class="unified">${layout("line-by-line")}</div><div class="split">${layout("side-by-side")}</div>`,
  };
}

/** Inner HTML for a diff fence with no hunk; lines are coloured by their first character. */
export function prefixHtml(patch: string): string {
  const kind = (l: string) => (l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : l.startsWith("@@") ? "hunk" : "ctx");
  const lines = patch.replace(/\n$/, "").split("\n");
  return `<code>${lines.map((l) => `<span class="dl ${kind(l)}">${esc(l)}\n</span>`).join("")}</code>`;
}
