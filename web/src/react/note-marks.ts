// Chat-note selectors (ADR 0162) that other modules skip: none of it is reply text. This file
// imports nothing, so notes-margin.tsx and find-text.ts can both use it.

/** A note card (notes-margin.tsx), and the Whole reply handle (transcript-row.tsx). */
export const NOTE_CARD = ".ncard";
export const REPLY_HANDLE = "[data-note-reply]";
