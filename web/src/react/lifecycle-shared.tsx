// Shared lifecycle dialog fields, frame, errors, and state subscription.

import { TriangleAlertIcon } from "lucide-react";
import { useSyncExternalStore, type ReactNode } from "react";
import type { App } from "../core/app";
import type { LifeState } from "../core/lifecycle";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../components/ui/dialog";

const FIELD = "h-8 w-full min-w-0 rounded-md border border-border-strong bg-background px-2 outline-none focus:border-ring disabled:opacity-60";
const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

function useLife<K extends keyof LifeState>(app: App, key: K): LifeState[K] {
  return useSyncExternalStore(app.life.subscribe, () => app.life.state[key]);
}

/** One dialog frame: a title, an optional line under it, the body, and a footer. */
function Frame({ open, onClose, title, description, children, testid, wide }: { open: boolean; onClose: () => void; title: string; description?: string; children: ReactNode; testid: string; wide?: boolean }) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      {open && (
        <DialogContent
          className={`${wide ? "w-[min(560px,calc(100vw-32px))]" : "w-[min(440px,calc(100vw-32px))]"} grid max-h-[calc(100vh-104px)] gap-3 overflow-y-auto p-4`}
          data-testid={testid}
          {...(description ? {} : { "aria-describedby": undefined })}
        >
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
          {children}
        </DialogContent>
      )}
    </Dialog>
  );
}

function Label({ text, children }: { text: string; children: ReactNode }) {
  return (
    <label className="grid gap-1 text-xs text-muted-foreground">
      {text}
      {children}
    </label>
  );
}

function ErrorLine({ text }: { text: string | null }) {
  if (!text) return null;
  return (
    <p role="alert" data-testid="dialog-error" className="flex items-start gap-1.5 text-sm break-words whitespace-pre-wrap text-failed select-text">
      <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" />
      {text}
    </p>
  );
}

export { FIELD, message, useLife, Frame, Label, ErrorLine };
