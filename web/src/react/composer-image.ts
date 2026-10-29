import type { ClipboardEvent } from "react";
import type { App } from "../core/app";
import { insertPendingImage, finishPendingImage } from "../core/images";

/** Keep the pending marker in the live draft while the server stores a pasted image. */
export function pasteImage(e: ClipboardEvent<HTMLTextAreaElement>, app: App, sessionId: string, change: (next: string) => void) {
  const image = [...e.clipboardData.files].find((f) => f.type.startsWith("image/"));
  if (!image) return;
  e.preventDefault();
  const el = e.currentTarget;
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
