// What chat notes (ADR 0161) put into a reply's markup: none of it is reply text. This file
// imports nothing, so notes-margin.tsx, transcript-overlays.tsx and find-text.ts can all use it.

/** A note card (notes-margin.tsx), and the Whole reply handle (transcript-row.tsx). */
export const NOTE_CARD = ".ncard";
export const REPLY_HANDLE = "[data-note-reply]";

/** The reply text of `node`, an element or a copied part of a range: a card's slot sits inside
 *  the reply's markup, but it is not reply text. */
export function replyText(node: Element | DocumentFragment): string {
  if (!node.querySelector(".note-under")) return node.textContent ?? "";
  const copy = node.cloneNode(true) as Element | DocumentFragment;
  for (const slot of copy.querySelectorAll(".note-under")) slot.remove();
  return copy.textContent ?? "";
}
