import type { KeyboardEvent } from "react";

/** Both model pickers keep Apply separate from choosing a result. */
export function modelPickerKey(e: KeyboardEvent, query: string) {
  if (e.nativeEvent.isComposing || e.metaKey || e.ctrlKey || e.altKey) return;
  const buttons = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("button[data-model-id]")];
  if (!buttons.length) return;
  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
  if (!(e.target instanceof HTMLInputElement) && index < 0) return;
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const next = index < 0 ? (e.key === "ArrowDown" ? 0 : buttons.length - 1) : (index + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
    buttons[next].focus();
  } else if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
    e.preventDefault();
    (buttons.find((b) => b.dataset.modelId === query.trim()) ?? buttons[0]).click();
  }
}
