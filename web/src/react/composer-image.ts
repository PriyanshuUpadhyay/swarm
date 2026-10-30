import type { ClipboardEvent, DragEvent } from "react";
import type { App } from "../core/app";
import { insertPendingImage, finishPendingImage } from "../core/images";

/** The types the server stores (`IMAGE_TYPES` in src/serve/images.rs). */
const STORED = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/** The first storable image in a drop, or why the drop is refused. */
export function pickImage(files: ArrayLike<File>): { image: File } | { refusal: string } {
  const image = Array.from(files).find((f) => STORED.includes(f.type));
  return image ? { image } : { refusal: "Only PNG, JPEG, GIF and WebP images can be attached" };
}

/** Keep the pending marker in the live draft while the server stores an image. */
function attach(image: File, el: HTMLTextAreaElement, app: App, sessionId: string, change: (next: string) => void) {
  const [start, end] = [el.selectionStart, el.selectionEnd];
  const marker = `[[Uploading image ${crypto.randomUUID()}]]`;
  change(insertPendingImage(app.draft(sessionId), start, end, marker));
  app.api.image(image).then(
    (path) => change(finishPendingImage(app.draft(sessionId), marker, path)),
    (err: unknown) => {
      change(finishPendingImage(app.draft(sessionId), marker, ""));
      app.notice(`Could not attach the image: ${err instanceof Error ? err.message : String(err)}`, true);
    },
  );
}

export function pasteImage(e: ClipboardEvent<HTMLTextAreaElement>, app: App, sessionId: string, change: (next: string) => void) {
  const image = [...e.clipboardData.files].find((f) => f.type.startsWith("image/"));
  if (!image) return;
  e.preventDefault();
  attach(image, e.currentTarget, app, sessionId, change);
}

/** A dropped file is always handled here, because left alone the webview navigates to it. */
export function dropImage(e: DragEvent<HTMLTextAreaElement>, app: App, sessionId: string, change: (next: string) => void) {
  if (!e.dataTransfer.types.includes("Files")) return;
  e.preventDefault();
  const picked = pickImage(e.dataTransfer.files);
  if ("refusal" in picked) app.notice(picked.refusal, true);
  else attach(picked.image, e.currentTarget, app, sessionId, change);
}

/** Without this the textarea is no drop target for a file, and the browser navigates to it. */
export function dragOverFiles(e: DragEvent) {
  if (e.dataTransfer.types.includes("Files")) e.preventDefault();
}
