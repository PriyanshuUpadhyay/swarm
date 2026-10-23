// Where the keyboard goes after a chat opens, or after the focused control goes away.

import { whenPainted } from "./perf";

const SIDEBAR = '[data-testid="sidebar"], [data-testid="recent"]';
const CHAT = ".strip-chat";

/** The place to type in a chat page or a pane: its question card, else its composer, else its
 *  transcript. A child's card in the chat page's Asking list is not the chat's own. */
export function inputOf(scope: Element): HTMLElement | null {
  return (
    scope.querySelector<HTMLElement>('.chat-input [data-testid="prompt-card"]') ??
    scope.querySelector<HTMLElement>('.chat-input [data-testid="composer"]:not(:disabled)') ??
    scope.querySelector<HTMLElement>('[data-testid="transcript"], [data-testid="pane-transcript"]')
  );
}

/** The pane or chat page that holds `el`; a dialog, a menu, a popover, or the tab strip gives the
 *  chat page. Other places, such as the Changes panel, keep their own focus. */
function scopeOf(el: Element | null): Element | null {
  if (!el) return null;
  const own = el.closest('[data-testid="pane"]') ?? el.closest(CHAT);
  if (own) return own;
  return el.closest('[role="dialog"], [role="alertdialog"], [role="menu"], [popover], [data-testid="tab-strip"]') ? document.querySelector(CHAT) : null;
}

/** Focuses the input of `el`'s pane or chat page, so the owner can type after a copy. */
export function focusInputOf(el: Element | null) {
  const scope = scopeOf(el);
  if (scope) inputOf(scope)?.focus();
}

/** After chat `session` paints, puts the keyboard in it. A key walk in the sidebar keeps the
 *  keyboard there, and a later open, a dialog, or a place the owner moved the keyboard to wins. */
export function focusOpenedChat(session: string, byKey: boolean, current: () => boolean) {
  // A pick in the palette leaves a closing dialog, which gives the keyboard back on its own.
  const before = document.activeElement?.closest('[role="dialog"]') ? null : document.activeElement;
  if (byKey && before?.closest(SIDEBAR)) return;
  const page = () => document.querySelector(`${CHAT}:has([data-testid="transcript"][data-chat="${CSS.escape(session)}"])`);
  whenPainted(
    () => !current() || (!document.querySelector('[role="dialog"]') && page() !== null),
    () => {
      const a = document.activeElement;
      const moved = before !== null && a !== before && a !== null && a !== document.body && a.isConnected;
      if (!current() || moved) return;
      const p = page();
      if (p) inputOf(p)?.focus();
    },
  );
}

/** A click or key in a chat page, a pane, or an overlay that leaves the keyboard on nothing (its
 *  control unmounted or went disabled) gives it back to that pane's input. Body keys stay for the
 *  sidebar's j and k, and the sidebar refocuses its own rows. */
export function keepFocus(): () => void {
  const after = (e: Event) => {
    const from = e.target instanceof Element ? e.target : null;
    if (!from || from === document.body || from.closest(SIDEBAR)) return;
    const scope = scopeOf(from);
    if (!scope) return;
    requestAnimationFrame(() => {
      const a = document.activeElement;
      const lost = !a || a === document.body || !a.isConnected || a.matches(":disabled");
      if (lost && scope.isConnected && !document.querySelector('[role="dialog"], [role="alertdialog"]')) inputOf(scope)?.focus();
    });
  };
  document.addEventListener("click", after, true);
  document.addEventListener("keydown", after, true);
  return () => {
    document.removeEventListener("click", after, true);
    document.removeEventListener("keydown", after, true);
  };
}
