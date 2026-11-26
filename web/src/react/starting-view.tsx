// A chat that is starting (ADR 0149): its steps as the `chat-start` frames report them, its setup
// output, and a box whose text goes after the first prompt.

import { CircleCheckIcon, LoaderCircleIcon, TriangleAlertIcon } from "lucide-react";
import type { App } from "../core/app";
import { hasPendingImage, imagePaths } from "../core/images";
import { setupFailure } from "../core/lifecycle";
import type { Starting, StartStep } from "../core/new-chat";
import { Button } from "../components/ui/button";
import { Kbd } from "../components/ui/kbd";
import { dragOverFiles, dropImage, pasteImage, type Draft } from "./composer-image";
import { ImageThumbs } from "./images";

export function StartingView({ app, start }: { app: App; start: Starting }) {
  // The steps come in the order the server sent them; a step with no words shows its name.
  return (
    <div className="flex flex-1 flex-col items-center gap-4 overflow-y-auto p-6" data-testid="starting-chat" data-state={start.error ? "failed" : "starting"}>
      <div className="grid w-full max-w-[720px] gap-3">
        <h2 className="truncate text-lg font-semibold">{start.title}</h2>
        {start.prompt && start.prompt !== start.title && <p className="line-clamp-4 text-sm whitespace-pre-wrap text-muted-foreground">{start.prompt}</p>}
        <ol className="grid gap-1.5" aria-label="Start steps" aria-live="polite" data-testid="start-steps">
          {start.steps.length === 0 && !start.error && <Step state="running" text="Getting ready" />}
          {start.steps.map((s) => (
            <Step key={s.step} state={s.state} text={`${s.words ?? s.step}${s.step === "worktree" && start.branch ? `: ${start.branch}` : ""}`} error={s.error} />
          ))}
        </ol>
        {start.setup && (
          <pre data-testid="setup-output" aria-label="Setup output" className="max-h-[260px] overflow-auto rounded-md border border-border bg-code p-2 font-mono text-xs break-words whitespace-pre-wrap select-text">
            {start.setup.output || (start.setup.state === "running" ? "Waiting for output…" : "(no output)")}
          </pre>
        )}
        {start.setup && start.setup.state !== "running" && start.setup.state !== "completed" && <p className="text-sm text-failed">{setupFailure(start.setup)}</p>}
        {start.error ? <Failed app={app} start={start} /> : <NextMessage app={app} start={start} />}
      </div>
    </div>
  );
}

function Failed({ app, start }: { app: App; start: Starting }) {
  return (
    <div className="grid gap-2">
      <p className="text-sm break-words text-failed" role="alert" data-testid="start-error">
        {start.error}
      </p>
      <div className="flex gap-2">
        <Button type="button" size="sm" data-testid="start-retry" onClick={() => app.newChat.retry(start.id)}>
          Edit and retry
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={() => app.newChat.dismiss(start.id)}>
          Close
        </Button>
      </div>
    </div>
  );
}

/** The box for the next message. Its text goes when the chat opens, also if not queued with ⌘↵. */
function NextMessage({ app, start }: { app: App; start: Starting }) {
  const waiting = start.queued.length;
  const box: Draft = { read: () => app.newChat.state.starts[start.id]?.draft ?? "", change: (next) => app.newChat.setDraft(start.id, next) };

  return (
    <div className="grid gap-1.5">
      <ImageThumbs paths={imagePaths(start.draft)} load={app.imageUrl} remove={(path) => box.change(start.draft.split(path).join("").replace(/  +/g, " "))} />
      <textarea
        rows={3}
        aria-label="Next message"
        data-testid="starting-prompt"
        placeholder="Type the next message. It goes after the first prompt."
        className="w-full resize-none rounded-lg border border-border bg-popover p-2 outline-none focus-visible:ring-2 focus-visible:ring-ring"
        value={start.draft}
        onChange={(e) => box.change(e.target.value)}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
            e.preventDefault();
            // A queued text with an upload marker would send the marker, not the image.
            if (!hasPendingImage(start.draft)) app.newChat.queue(start.id);
          }
        }}
        onPaste={(e) => pasteImage(e, app, box)}
        onDragOver={dragOverFiles}
        onDrop={(e) => dropImage(e, app, box)}
      />
      <p className="text-xs text-muted-foreground" role="status" data-testid="start-queued">
        <Kbd>⌘↵</Kbd> queues it; the box's text also goes when the chat opens.
        {waiting > 0 && <> {waiting === 1 ? "1 message waits." : `${waiting} messages wait.`}</>}
      </p>
    </div>
  );
}

function Step({ state, text, error }: { state: StartStep["state"]; text: string; error?: string | null }) {
  return (
    <li className="flex items-start gap-2 text-sm" data-state={state}>
      {state === "completed" ? (
        <CircleCheckIcon className="mt-0.5 size-4 text-done" aria-label="Done" />
      ) : state === "failed" ? (
        <TriangleAlertIcon className="mt-0.5 size-4 text-failed" aria-label="Failed" />
      ) : (
        <LoaderCircleIcon className="mt-0.5 size-4 animate-spin motion-reduce:animate-none" aria-label="Running" />
      )}
      <span className="min-w-0 break-words">
        {text}
        {error && <span className="block text-xs text-failed">{error}</span>}
      </span>
    </li>
  );
}
