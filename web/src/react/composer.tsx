import { ArrowUpIcon, ChevronDownIcon, CircleHelpIcon, LoaderCircleIcon, SquareIcon, TriangleAlertIcon } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { App, Banner, Composer, Input } from "../core/app";
import { modelLabel, outgoing, sends, usageLabel, type Prompt } from "../core/composer";
import { matches, menuAt, moved, offered, pick, picksAndSends, showsEmpty, type CommandList, type MenuRow } from "../core/slash";
import { Button } from "../components/ui/button";
import { cn } from "../lib/utils";

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
  // "the chair" means nothing to a new owner, so the chat's own agent is named.
  const target = chair.who ?? (chair.provider ? `the ${chair.provider} agent` : "the agent");
  const [text, setText] = useState(() => app.draft(sessionId));
  const box = useRef<HTMLTextAreaElement>(null);
  const [caret, setCaret] = useState(0);
  const [focused, setFocused] = useState(false);
  const [list, setList] = useState<CommandList | null>(null);
  const [selected, setSelected] = useState(0);
  // The start of the token whose menu Escape closed; the menu stays shut until the token changes.
  const [dismissed, setDismissed] = useState<number | null>(null);
  const pickedCaret = useRef<number | null>(null);
  const listId = useId();

  // A send or a pull-back changed the draft in the app.
  useEffect(() => setText(app.draft(sessionId)), [app, sessionId, state.draftRev]);

  // Grows with the text up to about 8 lines. After a pick the caret goes after the inserted text.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
    if (pickedCaret.current !== null) {
      el.setSelectionRange(pickedCaret.current, pickedCaret.current);
      pickedCaret.current = null;
    }
  }, [text]);

  const menu = focused && !chair.disabled ? menuAt(text, caret, chair.provider) : null;
  const rows = menu && list ? matches(offered(list.commands, menu), menu.query) : [];
  const open = menu !== null && dismissed !== menu.start && (rows.length > 0 || showsEmpty(menu));
  const current = Math.min(selected, Math.max(rows.length - 1, 0));
  const menuKey = menu ? `${menu.opener}${menu.start}` : null;

  // Each opening asks again; the server answers from its cache and refreshes a stale list.
  useEffect(() => {
    if (!menuKey) {
      setDismissed(null);
      return;
    }
    let live = true;
    app.menuCommands(sessionId).then(
      (next) => live && setList(next),
      (err: unknown) => live && setList((old) => ({ commands: old?.commands ?? [], note: err instanceof Error ? err.message : String(err) })),
    );
    return () => {
      live = false;
    };
  }, [app, sessionId, menuKey]);
  useEffect(() => {
    setSelected(0);
  }, [menuKey, menu?.query]);
  useEffect(() => {
    document.getElementById(`${listId}-${current}`)?.scrollIntoView({ block: "nearest" });
  }, [listId, current]);

  const change = (next: string) => {
    setText(next);
    app.setDraft(sessionId, next);
  };
  const canSend = outgoing(text) !== null && !state.sending && !state.pulling && !chair.disabled;
  const canPullBack = chair.pullBack && !chair.disabled && composer.queued > 0;
  const submit = () => canSend && void app.send(sessionId, text);

  /** A pasted image goes to the server, and its path goes in the draft at the caret. */
  const paste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const image = [...e.clipboardData.files].find((f) => f.type.startsWith("image/"));
    if (!image) return;
    e.preventDefault();
    const el = e.currentTarget;
    const [start, end] = [el.selectionStart, el.selectionEnd];
    app.api.image(image).then(
      (path) => {
        const draft = app.draft(sessionId);
        const before = draft.slice(0, start);
        const insert = `${before && !before.endsWith(" ") ? " " : ""}${path} `;
        change(before + insert + draft.slice(end));
        pickedCaret.current = start + insert.length;
      },
      (err: unknown) => app.notice(`Could not attach the image: ${err instanceof Error ? err.message : String(err)}`, true),
    );
  };

  /** Puts the row in the draft; `send` also sends that new draft at once. */
  const choose = (row: MenuRow, send: boolean) => {
    if (!menu) return;
    const next = pick(text, menu, row);
    change(next.text);
    if (send) return void app.send(sessionId, next.text);
    setCaret(next.caret);
    pickedCaret.current = next.caret;
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (open) {
      const row = rows[current];
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        setSelected(moved(current, rows.length, e.key === "ArrowDown" ? 1 : -1));
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setDismissed(menu.start);
        return;
      }
      if (row && e.key === "Tab" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        choose(row, false);
        return;
      }
      if (row && e.key === "Enter" && sends(e, sendKey)) {
        e.preventDefault();
        choose(row, picksAndSends(text, menu, row));
        return;
      }
    }
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
        className="relative rounded-lg border border-border-strong bg-surface shadow-sm focus-within:border-ring has-[textarea:disabled]:opacity-70"
      >
        {open && (
          <div
            id={listId}
            role="listbox"
            aria-label="Commands and skills"
            data-testid="slash-menu"
            className="absolute right-0 bottom-full left-0 z-20 mb-1 max-h-72 overflow-y-auto rounded-lg border border-border-strong bg-surface p-1 text-sm shadow-lg"
          >
            {rows.map((r, i) => (
              <div
                key={r.insert}
                id={`${listId}-${i}`}
                role="option"
                aria-selected={i === current}
                data-testid="slash-row"
                className={cn("flex cursor-default items-baseline gap-2 rounded-md px-2 py-1", i === current && "bg-hover")}
                onMouseMove={() => setSelected(i)}
                onMouseDown={(e) => {
                  // A click inserts only, and the text area keeps the focus.
                  e.preventDefault();
                  choose(r, false);
                }}
              >
                <span className="shrink-0 font-mono">{r.insert}</span>
                {r.hint && <span className="shrink-0 font-mono text-xs text-faint-foreground">{r.hint}</span>}
                <span className="min-w-0 flex-1 truncate text-muted-foreground">{r.description}</span>
                {r.dialog && (
                  <span
                    className="shrink-0 text-xs text-waiting"
                    title={`Opens a picker in the pane, so Swarm only inserts it. Run \`swarm attach ${chair.who ?? "orchestrator"}\` to use the picker.`}
                  >
                    opens in pane
                  </span>
                )}
                {r.source && <span className="shrink-0 text-xs text-faint-foreground">{r.source}</span>}
              </div>
            ))}
            {rows.length === 0 && (
              <div className="px-2 py-1 text-muted-foreground" data-testid="slash-status">
                {list ? "No matches" : "Loading commands…"}
              </div>
            )}
            {list?.note && <div className="px-2 py-1 text-xs text-failed">{list.note}</div>}
          </div>
        )}
        <textarea
          ref={box}
          rows={2}
          className="block max-h-[200px] w-full resize-none bg-transparent px-3 pt-2.5 text-base outline-none placeholder:text-faint-foreground"
          data-testid="composer"
          aria-label={`Message ${target}`}
          placeholder={chair.who ? `Message ${target}` : `Message ${target}. Press ⌘L to focus.`}
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-activedescendant={open && rows.length ? `${listId}-${current}` : undefined}
          value={text}
          onChange={(e) => {
            change(e.target.value);
            setCaret(e.target.selectionStart);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onKeyDown={onKeyDown}
          onPaste={paste}
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
              title={chair.who ? `Stop ${target}` : `Stop ${target} (⌘.)`}
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
      </div>
    </div>
  );
}
