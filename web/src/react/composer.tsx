import { ArrowUpIcon, Maximize2Icon, Minimize2Icon, SquareIcon } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { App, Composer } from "../core/app";
import { historyStep, type Browse } from "../core/history";
import { outgoing, sends, usageLabel } from "../core/composer";
import { editPicks, matches, menuAt, moved, offered, pick, picksAndSends, showsEmpty, skillFirst, skillNames, toSend, type CommandList, type MenuRow, type SkillPick } from "../core/slash";
import { Button } from "../components/ui/button";
import { ImageThumbs } from "./images";
import { RunChip } from "./run-chip";
import { cn } from "../lib/utils";

export function ComposerBox({ app, composer }: { app: App; composer: Composer }) {
  const { id: sessionId, agent: chair, state, sendKey } = composer;
  // "the chair" means nothing to a new owner, so the chat's own agent is named.
  const target = chair.who ?? (chair.provider ? `the ${chair.provider} agent` : "the agent");
  const [text, setText] = useState(() => app.draft(sessionId));
  const [picks, setPicks] = useState(() => app.draftPicks(sessionId));
  const box = useRef<HTMLTextAreaElement>(null);
  const highlights = useRef<HTMLDivElement>(null);
  const nativeEdit = useRef<{ start: number; end: number; inputType: string } | null>(null);
  const [caret, setCaret] = useState(0);
  const [focused, setFocused] = useState(false);
  const [list, setList] = useState<CommandList | null>(null);
  const [files, setFiles] = useState<CommandList | null>(null);
  const [selected, setSelected] = useState(0);
  // The start of the token whose menu Escape closed; the menu stays shut until the token changes.
  const [dismissed, setDismissed] = useState<number | null>(null);
  const pickedCaret = useRef<number | null>(null);
  const listId = useId();
  const browse = useRef<Browse | null>(null);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    const el = box.current;
    if (!el || chair.provider !== "pi") return;
    const before = (e: InputEvent) => {
      // History and composition can replace a range other than the visible selection.
      nativeEdit.current = e.isComposing || e.inputType.startsWith("history") ? null
        : { start: el.selectionStart, end: el.selectionEnd, inputType: e.inputType };
    };
    el.addEventListener("beforeinput", before);
    return () => el.removeEventListener("beforeinput", before);
  }, [chair.provider]);

  // Browsing ends without the draft coming back, so keep the draft where Esc finds it.
  const endBrowse = () => {
    const draft = browse.current?.draft;
    browse.current = null;
    if (draft?.trim()) app.setStash(sessionId, draft);
  };

  // A send, a pull-back or Reuse changed the draft in the app, which ends history browsing.
  useEffect(() => {
    endBrowse();
    setText(app.draft(sessionId));
    setPicks(app.draftPicks(sessionId));
  }, [app, sessionId, state.draftRev]);
  // A switch to another chat ends browsing too, so the draft is in the stash on return.
  useEffect(() => endBrowse, [app, sessionId]);

  // Grows with the text up to about 8 lines. After a pick the caret goes after the inserted text.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = expanded ? "" : `${Math.min(el.scrollHeight, 200)}px`;
    if (highlights.current) highlights.current.scrollTop = el.scrollTop;
    if (pickedCaret.current !== null) {
      el.setSelectionRange(pickedCaret.current, pickedCaret.current);
      pickedCaret.current = null;
    }
  }, [text, picks, expanded]);

  const menu = focused && !chair.disabled ? menuAt(text, caret, chair.provider) : null;
  const shownList = menu?.opener === "@" ? files : list;
  const rows = menu && shownList ? matches(offered(shownList.commands, menu, chair.provider), menu.query) : [];
  const open = menu !== null && dismissed !== menu.start && (rows.length > 0 || showsEmpty(menu, shownList));
  const current = Math.min(selected, Math.max(rows.length - 1, 0));
  const menuKey = menu ? `${menu.opener}${menu.start}` : null;

  // Each opening asks again; the server answers from its cache and refreshes a stale list.
  useEffect(() => {
    if (!menuKey) {
      setDismissed(null);
      return;
    }
    let live = true;
    // The file list stays apart, so pi's skill list keeps its rows for the send.
    const set = menuKey.startsWith("@") ? setFiles : setList;
    (menuKey.startsWith("@") ? app.menuFiles(sessionId) : app.menuCommands(sessionId)).then(
      (next) => live && set(next),
      (err: unknown) => live && set((old) => ({ commands: old?.commands ?? [], note: err instanceof Error ? err.message : String(err) })),
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

  const change = (next: string, recorded?: readonly SkillPick[]) => {
    setText(next);
    app.setDraft(sessionId, next, recorded);
    setPicks(app.draftPicks(sessionId));
  };
  // True while a pi send waits for pi's skill list, which can take the probe's 15 s deadline.
  const [listing, setListing] = useState(false);
  const canSend = outgoing(text) !== null && !state.sending && !listing && !state.pulling && !chair.disabled;
  const canPullBack = chair.pullBack && !chair.disabled && composer.queued > 0;
  const skills = skillNames(list);
  const loadingSkills = useRef(false);
  /** pi runs a skill only at the message's start, so the draft shows the moved text it sends. */
  const submit = async () => {
    if (!canSend || loadingSkills.current) return;
    endBrowse();
    loadingSkills.current = true;
    setListing(true);
    const out = await toSend(text, chair.provider, list, () => app.menuCommands(sessionId), picks).finally(() => {
      loadingSkills.current = false;
      setListing(false);
    });
    if (out.list !== list) {
      setList(out.list);
      // The owner typed while pi listed its skills, so this send is stale.
      if (app.draft(sessionId) !== text) return app.notice("Not sent, because the draft changed while pi listed its skills. Send again.");
    }
    // Keep the picked draft through a failed send. App.send clears it only after success.
    if (out.text !== text && !picks.length) change(out.text);
    void app.send(sessionId, out.text, picks.length ? text : out.text);
  };

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
    const recorded = editPicks(text, next.text, picks, menu);
    if (chair.provider === "pi" && row.kind === "skill") recorded.push({ start: menu.start, insert: row.insert });
    change(next.text, recorded);
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
        void submit();
      } else if (e.metaKey || e.ctrlKey) {
        // The text area adds no line for these by itself.
        e.preventDefault();
        const el = e.currentTarget;
        el.setRangeText("\n", el.selectionStart, el.selectionEnd, "end");
        change(el.value);
      }
    } else if (e.key === "Escape") {
      // Esc keeps one cleared draft, and Esc in an empty box gives it back.
      e.preventDefault();
      if (browse.current) {
        // The recalled prompt stays in the history, so the slot keeps the draft, or what it held.
        endBrowse();
        change("");
      } else if (text.trim()) {
        browse.current = null;
        app.setStash(sessionId, text);
        change("");
      } else if (text) {
        // Spaces alone are not worth the one stash slot.
        change("");
      } else if (app.stashed(sessionId)) {
        const back = app.stashed(sessionId);
        app.setStash(sessionId, "");
        change(back);
        pickedCaret.current = back.length;
      }
    } else if (e.key === "ArrowUp" && text === "" && canPullBack) {
      // The CLI's queue comes back before the history, as in Claude Code (ADR 0038).
      e.preventDefault();
      void app.pullBack(sessionId);
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const el = e.currentTarget;
      if (el.selectionStart !== el.selectionEnd) return;
      const step = historyStep(app.history(sessionId), browse.current, text, el.selectionStart, e.key === "ArrowUp" ? "up" : "down");
      if (!step) return;
      e.preventDefault();
      change(step.text);
      browse.current = step.browse;
      setCaret(step.caret);
      pickedCaret.current = step.caret;
      // A recalled prompt that ends in an @path or a /command opens no menu.
      setDismissed(menuAt(step.text, step.caret, chair.provider)?.start ?? null);
    }
  };

  const showStop = chair.running;
  const chips = chair.provider === "pi" ? picks : [];
  const extraSkills = chair.provider === "pi" ? skillFirst(text, skills, chips).extra : [];
  const caption = state.sending || listing
    ? "Sending…"
    : state.stopping
      ? "Stopping…"
      : state.pulling
        ? "Pulling back…"
        : state.alreadySent
          ? "Already sent"
          : canPullBack && text === ""
            ? "↑ to edit queued messages"
            : text === "" && app.stashed(sessionId)
              ? "Draft cleared. Esc brings it back."
            : extraSkills.length
              ? `pi runs one skill per message, so ${extraSkills.join(", ")} goes as text`
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
            aria-label={menu.opener === "@" ? "Files" : "Commands and skills"}
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
                {/* A file path can be wider than the menu, so it wraps; a command name never does. */}
                <span className={cn("font-mono", r.kind === "file" ? "min-w-0 break-all" : "shrink-0")}>{r.insert}</span>
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
                {shownList ? "No matches" : menu.opener === "@" ? "Loading files…" : "Loading commands…"}
              </div>
            )}
            {shownList?.note && <div className="px-2 py-1 text-xs text-failed">{shownList.note}</div>}
          </div>
        )}
        <ImageThumbs text={text} load={app.imageUrl} remove={(path) => change(text.split(path).join("").replace(/  +/g, " "))} />
        <div className="relative">
          {chips.length > 0 && (
            <div ref={highlights} aria-hidden="true" data-testid="composer-highlights" className="composer-text composer-highlights">
              {chips.map((p, i) => <span key={p.start}>
                {text.slice(i ? chips[i - 1].start + chips[i - 1].insert.length : 0, p.start)}
                <span data-testid="composer-skill-chip" data-muted={i > 0} className="composer-skill-chip">{p.insert}</span>
              </span>)}
              {text.slice(chips.at(-1)!.start + chips.at(-1)!.insert.length)}{"\n"}
            </div>
          )}
          <textarea
            ref={box}
            rows={2}
            className={cn("block w-full", expanded ? "h-[60vh]" : "max-h-[200px]", "resize-none bg-transparent px-3 pt-2.5 text-base outline-none placeholder:text-faint-foreground", chair.provider === "pi" && "composer-text relative")}
            data-testid="composer"
            aria-label={`Message ${target}`}
            placeholder={chair.who ? `Message ${target}` : `Message ${target}. Press ⌘L to focus.`}
            aria-expanded={open}
            aria-controls={open ? listId : undefined}
            aria-activedescendant={open && rows.length ? `${listId}-${current}` : undefined}
            value={text}
            onChange={(e) => {
              const edit = nativeEdit.current;
              nativeEdit.current = null;
              if (edit && edit.start === edit.end && edit.inputType.startsWith("delete")) {
                const removed = text.length - e.target.value.length;
                if (edit.inputType.endsWith("Backward")) edit.start -= removed;
                else edit.end += removed;
              }
              // An edit ends browsing.
              endBrowse();
              change(e.target.value, editPicks(text, e.target.value, picks, edit ?? undefined));
              setCaret(e.target.selectionStart);
            }}
            onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            onKeyDown={onKeyDown}
            onPaste={paste}
            onScroll={(e) => {
              if (!highlights.current) return;
              highlights.current.scrollTop = e.currentTarget.scrollTop;
              highlights.current.scrollLeft = e.currentTarget.scrollLeft;
            }}
          />
        </div>
        <div className="flex items-center gap-2 px-2 pb-2 text-xs text-muted-foreground">
          <RunChip app={app} sessionId={sessionId} chair={chair} />
          <button type="button" data-testid="usage" className="rounded-md px-1.5 py-1 tabular-nums hover:bg-hover" title="Context, cache, and estimated cost for this agent session">
            {usageLabel(composer.usage)}
          </button>
          {showStop && !chair.who && <span className="text-faint-foreground">⌘. stops</span>}
          <span className="flex-1" />
          <Button
            variant="ghost"
            size="icon"
            data-testid="composer-expand"
            aria-label={expanded ? "Shrink composer" : "Expand composer"}
            title={expanded ? "Shrink composer" : "Expand composer"}
            onClick={() => {
              setExpanded(!expanded);
              box.current?.focus();
            }}
          >
            {expanded ? <Minimize2Icon className="size-3.5" /> : <Maximize2Icon className="size-3.5" />}
          </Button>
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
      <div role="status" className="flex min-h-5 items-center gap-2 px-1 pt-1 text-xs text-muted-foreground" data-testid="composer-status">
        <span className="flex-1">{chair.disabled ?? caption}</span>
      </div>
    </div>
  );
}
