// The lifecycle dialogs (S3): New Workspace with its setup output, Create and Import Project, the
// project setup script, Switch model with its progress, one confirm dialog, and Archived chats.
// Each reads only its own field of `app.life.state`.

import { LoaderCircleIcon } from "lucide-react";
import { useEffect, useState } from "react";
import type { App } from "../core/app";
import { Button } from "../components/ui/button";
import { useLife, Frame, ErrorLine } from "./lifecycle-shared";
import { NewWorkspaceDialog } from "./lifecycle-workspace";
import { ProjectDialog, SetupScriptDialog } from "./lifecycle-project";
import { SwitchModelDialog } from "./lifecycle-switch-model";
import { ClosedDialog, ImportDialog } from "./lifecycle-chats";

export function LifecycleDialogs({ app }: { app: App }) {
  return (
    <>
      <NewWorkspaceDialog app={app} />
      <ProjectDialog app={app} />
      <SetupScriptDialog app={app} />
      <SwitchModelDialog app={app} />
      <ConfirmDialog app={app} />
      <ClosedDialog app={app} />
      <ImportDialog app={app} />
    </>
  );
}

// ---- confirm ----

function ConfirmDialog({ app }: { app: App }) {
  const confirm = useLife(app, "confirm");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => setError(null), [confirm]);
  return (
    <Frame open={confirm !== null} onClose={() => !busy && app.life.close("confirm")} title={confirm?.title ?? ""} testid="confirm">
      {confirm && (
        <>
          <p className="text-sm break-words whitespace-pre-wrap select-text" data-testid="confirm-body">
            {confirm.body}
          </p>
          <ErrorLine text={error} />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={busy} onClick={() => app.life.close("confirm")}>
              Cancel
            </Button>
            <Button
              type="button"
              size="sm"
              data-testid="confirm-action"
              className="bg-failed text-white hover:bg-failed/90"
              disabled={busy || confirm.blocked}
              onClick={async () => {
                setBusy(true);
                setError(await app.life.confirmed());
                setBusy(false);
              }}
            >
              {busy && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
              {confirm.action}
            </Button>
          </div>
        </>
      )}
    </Frame>
  );
}
