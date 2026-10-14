import { ArrowUpIcon, ChevronDownIcon, CircleHelpIcon, LoaderCircleIcon, SquareIcon, TriangleAlertIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { App, Banner, Composer, Input } from "../core/app";
import { modelLabel, outgoing, sends, usageLabel, type Prompt } from "../core/composer";
import { Button } from "../components/ui/button";
import { Kbd } from "../components/ui/kbd";

/** Below the transcript: the error banner, the agent's question, and the composer. */
export function ChatInput({ app, composer }: { app: App; composer: Composer }) {
  const { id, agent, state } = composer;
  return (
    <div className="chat-input shrink-0 px-6 pt-1 pb-4">
      <div className="mx-auto grid max-w-[820px] gap-2">
        {state.banner && <ChatBanner app={app} sessionId={id} banner={state.banner} />}
        {agent.prompt && <PromptCard app={app} sessionId={id} who={agent.who} prompt={agent.prompt} state={state} />}
        {/* A child with no pane shows only the reason (ChildColumnView). A chair with none shows
            only its log, and Continue chat starts a new chat from its summary (ADR 0083). */}
        {agent.who && agent.unreachable ? (
          <p className="text-xs text-muted-foreground" data-testid="composer-status">
            {agent.disabled}
          </p>
        ) : agent.unreachable ? (
          <div className="flex justify-center">
            <Button variant="outline" size="sm" data-testid="continue-chat" onClick={() => app.life.openSwitchModel(id)}>
              Continue chat
            </Button>
          </div>
        ) : (
          <ComposerBox key={id} app={app} composer={composer} />
        )}
      </div>
    </div>
  );
}

function ChatBanner({ app, sessionId, banner }: { app: App; sessionId: string; banner: Banner }) {
  return (
    <div role="alert" data-testid="chat-banner" className="flex items-start gap-2 rounded-lg border border-failed/40 bg-failed/10 px-3 py-2 text-sm">
      <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-failed" />
      <span className="flex-1 font-mono text-xs leading-5 break-words whitespace-pre-wrap">{banner.text}</span>
      <Button variant="outline" size="sm" onClick={banner.retry}>
        Retry
      </Button>
      <Button variant="ghost" size="sm" onClick={() => app.dismissBanner(sessionId)}>
        Dismiss
      </Button>
    </div>
  );
}

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
        {prompt.choices.map((label, i) => (
          <button
            key={i}
            type="button"
            data-testid="prompt-choice"
            disabled={busy !== null}
            onClick={() => pick(i)}
            className="flex items-start gap-2 rounded-md border border-border-strong bg-background px-2.5 py-1.5 text-left text-sm hover:bg-hover disabled:opacity-60"
          >
            <span className="font-mono text-muted-foreground tabular-nums">{i + 1}</span>
            <span className="flex-1 whitespace-pre-wrap">{label}</span>
            {busy === i && <LoaderCircleIcon className="size-3.5 animate-spin" />}
          </button>
        ))}
      </div>
    </div>
  );
}

function ComposerBox({ app, composer }: { app: App; composer: Composer }) {
  const { id: sessionId, agent: chair, state, sendKey } = composer;
  const target = chair.who ?? "the chair";
  const [text, setText] = useState(() => app.draft(sessionId));
  const box = useRef<HTMLTextAreaElement>(null);

  // A send or a pull-back changed the draft in the app.
  useEffect(() => setText(app.draft(sessionId)), [app, sessionId, state.draftRev]);

  // Grows with the text up to about 8 lines.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
  }, [text]);

  const change = (next: string) => {
    setText(next);
    app.setDraft(sessionId, next);
  };
  const canSend = outgoing(text) !== null && !state.sending && !state.pulling && !chair.disabled;
  const canPullBack = chair.pullBack && !chair.disabled && composer.queued > 0;
  const submit = () => canSend && void app.send(sessionId, text);

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    // ⇧⌘↩ zooms the pane.
    if (e.key === "Enter" && e.shiftKey && e.metaKey) return;
    if (e.key === "Enter") {
      if (sends(e, sendKey)) {
        e.preventDefault();
        submit();
      } else if (e.metaKey || e.ctrlKey) {
        // The text area adds no line for these by itself.
        e.preventDefault();
        const el = e.currentTarget;
        el.setRangeText("\n", el.selectionStart, el.selectionEnd, "end");
        change(el.value);
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      change("");
    } else if (e.key === "ArrowUp" && text === "" && canPullBack) {
      e.preventDefault();
      void app.pullBack(sessionId);
    }
  };

  const showStop = chair.running;
  const caption = state.sending
    ? "Sending…"
    : state.stopping
      ? "Stopping…"
      : state.pulling
        ? "Pulling back…"
        : state.alreadySent
          ? "Already sent"
          : canPullBack && text === ""
            ? "↑ to edit queued messages"
            : null;

  return (
    <div>
      <div
        data-testid="composer-box"
        className="rounded-lg border border-border-strong bg-surface shadow-sm focus-within:border-ring has-[textarea:disabled]:opacity-70"
      >
        <textarea
          ref={box}
          rows={2}
          className="block max-h-[200px] w-full resize-none bg-transparent px-3 pt-2.5 text-base outline-none placeholder:text-faint-foreground"
          data-testid="composer"
          aria-label={`Message ${target}`}
          placeholder={`Message ${target}`}
          value={text}
          onChange={(e) => change(e.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="flex items-center gap-2 px-2 pb-2 text-xs text-muted-foreground">
          {/* A child's model does not switch. */}
          <button
            type="button"
            data-testid="model-chip"
            className="flex min-w-0 items-center gap-1 rounded-md px-1.5 py-1 hover:bg-hover disabled:pointer-events-none"
            title={chair.who ? undefined : "Switch model"}
            disabled={chair.who !== null}
            onClick={() => app.life.openSwitchModel(sessionId)}
          >
            <span className="truncate">{modelLabel(chair)}</span>
            <ChevronDownIcon className="size-3 shrink-0" />
          </button>
          <button type="button" data-testid="usage" className="rounded-md px-1.5 py-1 tabular-nums hover:bg-hover" title="Context, cache, and estimated cost for this agent session">
            {usageLabel(composer.usage)}
          </button>
          {showStop && !chair.who && <span className="text-faint-foreground">⌘. stops</span>}
          <span className="flex-1" />
          {showStop && (
            <Button
              variant="outline"
              size="icon"
              data-testid="stop"
              aria-label={`Stop ${target}`}
              title={chair.who ? `Stop ${target}` : "Stop the chair (⌘.)"}
              className="rounded-full text-failed"
              disabled={state.stopping}
              onClick={() => void app.interrupt(sessionId)}
            >
              <SquareIcon className="size-3 fill-current" />
            </Button>
          )}
          {(!showStop || outgoing(text) !== null) && (
            <Button size="icon" data-testid="send" aria-label="Send" title={sendKey === "return" ? "Send (Return)" : "Send (⌘Return)"} disabled={!canSend} onClick={submit}>
              <ArrowUpIcon className="size-4" />
            </Button>
          )}
        </div>
      </div>
      <div className="flex min-h-5 items-center gap-2 px-1 pt-1 text-xs text-muted-foreground" data-testid="composer-status">
        <span className="flex-1">{chair.disabled ?? caption}</span>
        {!chair.who && <Kbd>⌘L</Kbd>}
      </div>
    </div>
  );
}
