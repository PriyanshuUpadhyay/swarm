import { CircleHelpIcon, LoaderCircleIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import type { App, Input } from "../core/app";
import type { Prompt } from "../core/composer";

/** An agent's question, one button per choice in the CLI's words (ADR 0029). Nothing is picked
 *  for the owner. While the card has focus, 1 to 9 pick a choice, as in the CLI. `sessionId` is the
 *  chat id, and `who` the child agent's id (null for the chair). */
export function PromptCard({
  app,
  sessionId,
  who,
  prompt,
  state,
  showColumn,
}: {
  app: App;
  sessionId: string;
  who: string | null;
  prompt: Prompt;
  state: Input;
  showColumn?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const busy = state.answering?.prompt === prompt.id ? state.answering.choice : null;
  // The composer cannot send while the question is open, so the card takes its focus. The chair's
  // card also takes the keyboard when nothing has it. A child's question takes it from nothing
  // else, so it never moves the owner out of the chat or out of another pane.
  useEffect(() => {
    const a = document.activeElement;
    const own = a?.matches('[data-testid="composer"]') && ref.current?.closest(".chat-input")?.contains(a);
    if (own || (!who && (!a || a === document.body))) ref.current?.focus();
  }, [prompt.id]);
  // App state shows the answer's result, so this handler does not await the promise.
  const pick = (i: number) => void app.answer(sessionId, prompt, i);
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || !/^[1-9]$/.test(e.key)) return;
    const i = Number(e.key) - 1;
    if (i >= prompt.choices.length) return;
    e.preventDefault();
    pick(i);
  };
  return (
    <div
      ref={ref}
      tabIndex={0}
      data-testid="prompt-card"
      aria-label={`${who ?? "The chair"} is waiting: ${prompt.question}`}
      className="grid gap-2 rounded-lg border border-waiting/40 bg-waiting/10 p-3 outline-none focus-visible:ring-1 focus-visible:ring-ring"
      onKeyDown={onKeyDown}
    >
      <div className="flex items-center gap-1.5 text-xs font-semibold text-waiting">
        <CircleHelpIcon className="size-3.5" />
        <span className="flex-1">{who ?? "The chair"} is waiting for you</span>
        {showColumn && (
          <button type="button" className="t-link font-normal" onClick={showColumn}>
            Show column
          </button>
        )}
      </div>
      <div className="text-sm whitespace-pre-wrap select-text">{prompt.question}</div>
      <div className="grid gap-1">
        {prompt.choices.map((choice, i) => {
          const detail = prompt.details?.[i];
          return (
            <button
              key={i}
              type="button"
              data-testid="prompt-choice"
              title={detail ? `${choice}\n${detail}` : choice}
              disabled={busy !== null}
              onClick={() => pick(i)}
              className="flex items-start gap-2 rounded-md border border-border-strong bg-background px-2.5 py-1.5 text-left text-sm hover:bg-hover disabled:opacity-60"
            >
              <span className="font-mono text-muted-foreground tabular-nums">{i + 1}</span>
              <span className="min-w-0 flex-1 break-words">
                <span className="block whitespace-pre-wrap">{choice}</span>
                {detail && <span className="line-clamp-2 text-xs whitespace-pre-wrap text-muted-foreground">{detail}</span>}
              </span>
              {busy === i && <LoaderCircleIcon className="size-3.5 animate-spin" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}

