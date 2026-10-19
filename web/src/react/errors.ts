import { toast } from "sonner";

/** main.tsx sets this to send each shown error to the server's `errors.jsonl`. */
export const shownErrors = { report: (_text: string) => {} };

/** Every error toast goes through here, so each one is recorded for diagnosis. */
export function showError(text: string) {
  toast.error(text);
  shownErrors.report(text);
}
