import { toast } from "sonner";
import { showError } from "./errors";

/** Copies text, or text still being fetched. A pending text goes in a ClipboardItem promise, so
 *  WebKit keeps the click's user activation across the fetch. */
export async function copyText(text: string | Promise<string>, what: string) {
  try {
    if (typeof text === "string") await navigator.clipboard.writeText(text);
    else await navigator.clipboard.write([new ClipboardItem({ "text/plain": text.then((t) => new Blob([t], { type: "text/plain" })) })]);
    toast(`Copied ${what}`);
  } catch {
    showError(`Could not copy the ${what}`);
  }
}
