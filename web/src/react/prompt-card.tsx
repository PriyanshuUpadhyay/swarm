import { CircleHelpIcon, LoaderCircleIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { App, Input } from "../core/app";
import type { Prompt } from "../core/composer";
import { Kbd } from "../components/ui/kbd";
import { cn } from "../lib/utils";
import { CutLine } from "./cut-line";

const COMMAND_PREVIEW_LINES = 4;

function permissionCommand(prompt: Prompt) {
  if (prompt.screen || prompt.input) return null;
  const [title, ...rows] = prompt.question.split("\n");
  // Claude marks each Bash command row with │. Do not cut unmarked prompt text.
  if (title !== "Bash command") return null;
  const lines = [];
  for (const row of rows) {
    if (row !== "│" && !row.startsWith("│ ")) break;
    lines.push(row.slice(2));
  }
  return lines.length ? { title, lines, rest: rows.slice(lines.length).join("\n") } : null;
}

/** An agent's question, one button per choice in the CLI's words (ADR 0029). Nothing is picked
 *  for the owner. While the card has focus, 1 to 9 pick a choice, as in the CLI. A question that
 *  takes typed text gets one text line and a Send button instead. A screen card (ADR 0159) shows
 *  the agent's pane rows with one button per key and a text line; it takes no digit keys, because
 *  its buttons are keys, not choices. `sessionId` is the chat id, and `who` the child agent's id
 *  (null for the chair). */
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
  const rows = useRef<HTMLPreElement>(null);
  const commandWindow = useRef<HTMLPreElement>(null);
  const [typed, setTyped] = useState({ prompt: prompt.id, text: "" });
  const [expanded, setExpanded] = useState<string | null>(null);
  const command = permissionCommand(prompt);
  const commandId = `${sessionId}:${prompt.id}`;
  const showAll = expanded === commandId;
  const text = typed.prompt === prompt.id ? typed.text : "";
  const busy = state.answering?.prompt === prompt.id ? state.answering.choice : null;

  // The question of a screen card is at the pane's bottom, so its rows open scrolled to the end.
  useEffect(() => {
    if (rows.current) rows.current.scrollTop = rows.current.scrollHeight;
  }, [prompt.id]);

  const commandCut = !showAll && command !== null && command.lines.length > COMMAND_PREVIEW_LINES;
  const showCommand = () => {
    setExpanded(commandId);
    commandWindow.current?.focus({ preventScroll: true });
  };

  const pick = (i: number) => {
    if (commandCut) showCommand();
    // App state shows the answer result, so the picker does not wait here.
    else void app.answer(sessionId, prompt, i);
  };

  // The CLI's input row is one line, so the text goes as one line.
  const line = text.replace(/\s+/g, " ").trim();
  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (line && busy === null) void app.answer(sessionId, prompt, { text: line });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (prompt.screen || e.target instanceof HTMLInputElement || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || !/^[1-9]$/.test(e.key)) return;
    const i = Number(e.key) - 1;
    if (i >= prompt.choices.length) return;
    e.preventDefault();
    // Holding a digit must not both reveal the command and answer the prompt.
    if (!command || !e.repeat) pick(i);
  };

  const answerAfterChoices = !prompt.screen && prompt.choices.length > 0;
  const answer = prompt.input && (
    <form className="flex min-w-0 items-center gap-2" onSubmit={submit}>
      <input
        type="text"
        name="answer"
        autoComplete="off"
        spellCheck={false}
        data-testid="prompt-text"
        aria-label="Your answer"
        placeholder={prompt.screen ? "Type text; Send types it and presses Enter" : "Or type your own answer…"}
        value={text}
        disabled={busy !== null}
        onChange={(e) => setTyped({ prompt: prompt.id, text: e.target.value })}
        className="min-w-0 flex-1 rounded-md border border-border bg-background px-2.5 py-1.5 text-sm pointer-coarse:text-[16px] outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-60"
      />

      <button
        type="submit"
        data-testid="prompt-send"
        disabled={busy !== null || !line}
        className="flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-sm hover:bg-hover disabled:opacity-60"
      >
        Send
        {busy === -1 && <LoaderCircleIcon className="size-3.5 animate-spin motion-reduce:animate-none" />}
      </button>
    </form>
  );

  return (
    <div
      tabIndex={0}
      role="group"
      data-testid="prompt-card"
      aria-label={`${who ?? "The chair"} is waiting: ${prompt.screen ? "its screen is shown below" : prompt.question}`}
      className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-2 rounded-xl border border-border bg-surface p-4 outline-none focus-visible:ring-1 focus-visible:ring-ring"
      onKeyDown={onKeyDown}
    >
      <div className={cn("flex items-center gap-1.5 text-xs font-semibold", command ? "text-muted-foreground" : "text-waiting")}>
        {command ? <span className="mx-0.5 size-2 shrink-0 rounded-full bg-waiting" aria-hidden /> : <CircleHelpIcon className="size-3.5" />}
        <span className="flex-1">{who ?? "The chair"} is waiting for you</span>
        {showColumn && (
          <button type="button" className="t-link font-normal" onClick={showColumn}>
            Show column
          </button>
        )}
      </div>
      {/* A long question, such as a plan to approve, scrolls so the choices stay in view. */}
      {prompt.screen ? (
        <pre
          ref={rows}
          data-testid="prompt-question"
          className="max-h-[40vh] min-w-0 overflow-auto overscroll-contain rounded-md bg-background p-2 font-mono text-xs leading-snug whitespace-pre select-text"
        >
          {prompt.question}
        </pre>
      ) : command ? (
        <div data-testid="prompt-question" className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-2 text-md break-words select-text">
          <span className="font-semibold">{command.title}</span>
          <div className="min-w-0 rounded-lg border border-border bg-background px-3 py-2">
            <pre ref={commandWindow} data-testid="permission-command" tabIndex={showAll ? 0 : -1} className="max-h-[min(20lh,40vh)] overflow-auto overscroll-contain font-mono text-sm leading-[18px] whitespace-pre">{(showAll ? command.lines : command.lines.slice(0, COMMAND_PREVIEW_LINES)).join("\n")}</pre>
            {commandCut && <CutLine count={`${command.lines.length - COMMAND_PREVIEW_LINES} more`} show={showCommand} />}
          </div>
          {command.rest && <span className="min-w-0 whitespace-pre-wrap">{command.rest}</span>}
        </div>
      ) : (
        <div data-testid="prompt-question" className="max-h-[40vh] min-w-0 overflow-y-auto overscroll-contain text-md font-semibold break-words whitespace-pre-wrap select-text">
          {prompt.question}
        </div>
      )}
      {!answerAfterChoices && answer}
      <div className={cn("grid gap-1", prompt.screen && "grid-cols-4")}>
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
              className="flex min-h-[34px] min-w-0 items-start gap-2 rounded-lg border border-border bg-background px-2.5 py-1.5 text-left text-sm hover:bg-hover disabled:opacity-60"
            >
              {!prompt.screen && (i < 9 ? <Kbd>{i + 1}</Kbd> : <span className="min-w-5 text-center font-mono text-muted-foreground tabular-nums">{i + 1}</span>)}
              <span className="min-w-0 flex-1 break-words">
                <span className="block whitespace-pre-wrap">{choice}</span>
                {detail && <span className="line-clamp-2 text-xs whitespace-pre-wrap text-muted-foreground">{detail}</span>}
              </span>
              {busy === i && <LoaderCircleIcon className="size-3.5 animate-spin motion-reduce:animate-none" />}
            </button>
          );
        })}
      </div>
      {answerAfterChoices && answer}
    </div>
  );
}

