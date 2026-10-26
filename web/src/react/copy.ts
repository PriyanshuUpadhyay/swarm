import { toast } from "sonner";
import { showError } from "./errors";
import { focusInputOf } from "../core/focus";

/** Copies text, or text still being fetched. A pending text goes in a ClipboardItem promise, so
 *  WebKit keeps the click's user activation across the fetch. A copy from a button in a chat (`from`)
 *  gives the keyboard back to that chat's input, so the owner can paste at once. */
export async function copyText(text: string | Promise<string>, what: string, from?: Element) {
  try {
    if (from) focusInputOf(from);
    if (typeof text === "string") await navigator.clipboard.writeText(text);
    else await navigator.clipboard.write([new ClipboardItem({ "text/plain": text.then((t) => new Blob([t], { type: "text/plain" })) })]);
    toast(`Copied ${what}`);
  } catch {
    showError(`Could not copy the ${what}`);
  }
}
