import { ArrowDownIcon, ChevronDownIcon, ChevronUpIcon, MessageSquareDashedIcon, TriangleAlertIcon, XIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { Virtualizer, type VirtualizerHandle } from "virtua";
import { PROMPT_JUMP, SENT, type App, type Pane, type Reader } from "../core/app";
import { canBranch } from "../core/composer";
import { promptIndex, type Line } from "../core/transcript";
import { ContextMenu, ContextMenuTrigger } from "../components/ui/context-menu";
import { Kbd } from "../components/ui/kbd";
import { ChatInput } from "./chat-input";
import { copyText } from "./copy";
import { focusInputOf } from "../core/focus";
import { SourcePopover, TranscriptMenu, type Source } from "./transcript-overlays";
import { setSplit } from "./transcript-output";
import { paintRanges, textRanges } from "./find-text";
import { LineView } from "./transcript-row";

const readingPlaces = new Map<string, { row: string; offset: number; bottom: boolean }>();

export { StatusGlyph } from "./transcript-tool";

/** The chat column: transcript, its empty, loading and error states, and the composer. */
export function ChatView({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  return (
    <>
      <Transcript app={app} view={view} empty={view.openId ? "No messages yet. Send the first one below." : undefined} />
      {view.composer && <ChatInput app={app} composer={view.composer} />}
    </>
  );
}

/** A child agent's column body: the same transcript and composer on the child's chat (ADR 0029). */
export function PaneBody({ app, pane, empty }: { app: App; pane: Pane; empty: string }) {
  return (
    <>
      <Transcript app={app.reader(pane.id)} view={pane} testid="pane-transcript" empty={empty} />
      {pane.composer && <ChatInput app={app} composer={pane.composer} />}
    </>
  );
}

/** The fixed-height line above the rows; the Virtualizer starts below it. */
const HEAD_HEIGHT = 36;

function Transcript({ app, view, testid = "transcript", empty }: { app: Reader; view: Pane; testid?: string; empty?: string }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const handle = useRef<VirtualizerHandle>(null);
  const atBottom = useRef(true);
  const [away, setAway] = useState(false);
  const [source, setSource] = useState<Source | null>(null);
  const { lines, id: openId, chatState, find } = view;
  const current = find.open ? find.matches[find.index]?.id : undefined;

  // A new chat, or its first page: start at the newest row.
  useLayoutEffect(() => {
    if (chatState !== "ready") return;
    const place = readingPlaces.get(openId);
    atBottom.current = place?.bottom ?? true;
    setAway(!atBottom.current);
    const index = place && !place.bottom ? lines.findIndex((l) => l.id === place.row) : -1;
    if (index >= 0) {
      handle.current?.scrollToIndex(index, { align: "start", offset: -place!.offset });
    } else if (lines.length) handle.current?.scrollToIndex(lines.length - 1, { align: "end" });
  }, [openId, chatState]);

  // New or changed rows: follow only when the reader is at the bottom (ADR 0028).
  useLayoutEffect(() => {
    if (!view.shift && atBottom.current && lines.length) handle.current?.scrollToIndex(lines.length - 1, { align: "end" });
  }, [lines, view.shift]);

  // A find step brings its match into view. Declared after the follow effect, so it wins.
  useLayoutEffect(() => {
    if (!current) return;
    const i = lines.findIndex((l) => l.id === current);
    if (i >= 0) handle.current?.scrollToIndex(i, { align: "center" });
    let frame = 0;
    const timer = requestAnimationFrame(() => {
      const match = find.matches[find.index];
      const row = scrollRef.current?.querySelector<HTMLElement>(`[data-id="${CSS.escape(current)}"]`);
      if (match?.preview) row?.dispatchEvent(new CustomEvent("swarm:find-output", { bubbles: true, detail: match }));
      frame = requestAnimationFrame(() => {
        if (!row || !match) return;
        const target = match.preview ? row.querySelector<HTMLElement>(`[data-preview="${CSS.escape(match.preview)}"] [data-output-line="${match.line}"]`) ?? row.querySelector<HTMLElement>(`[data-preview="${CSS.escape(match.preview)}"] .patch`) ?? row : row;
        const ranges = textRanges(target, find.query);
        const selected = ranges[match.preview ? match.lineOccurrence ?? 0 : match.occurrence];
        paintRanges("find-matches", textRanges(row, find.query));
        paintRanges("find-current", selected ? [selected] : []);
        const rect = selected?.getBoundingClientRect();
        if (rect && scrollRef.current) handle.current?.scrollToIndex(i, { align: "start", offset: rect.top - row.getBoundingClientRect().top - scrollRef.current.clientHeight / 2 });
      });
    });
    return () => { cancelAnimationFrame(timer); cancelAnimationFrame(frame); };
  }, [find.seq]);
  useEffect(() => {
    if (!find.open) { paintRanges("find-matches", []); paintRanges("find-current", []); }
  }, [find.open]);

  const onScroll = (offset: number) => {
    const h = handle.current;
    if (!h) return;
    atBottom.current = offset + h.viewportSize >= h.scrollSize - 8;
    const first = h.findItemIndex(offset);
    const el = scrollRef.current?.querySelector<HTMLElement>(`[data-index="${first}"]`);
    if (lines[first]) readingPlaces.set(openId, { row: lines[first].id, offset: el && scrollRef.current ? el.getBoundingClientRect().top - scrollRef.current.getBoundingClientRect().top : 0, bottom: atBottom.current });
    setAway(!atBottom.current && h.scrollSize - offset - h.viewportSize > 400);
    if (offset < 400) void app.loadOlder();
  };

  // ⌃⌥↑ and ⌃⌥↓ (App's prompt.prev and prompt.next) move between the owner's prompts in the chat.
  useEffect(() => {
    if (testid !== "transcript") return;
    const onJump = (e: Event) => {
      const h = handle.current;
      const scroller = scrollRef.current;
      if (!h || !scroller) return;
      const dir = (e as CustomEvent<1 | -1>).detail;
      // The first row with a part in view. A prompt cut at the top is still above, so ⌃⌥↑ goes to it.
      const viewTop = scroller.getBoundingClientRect().top;
      const first = [...scroller.querySelectorAll<HTMLElement>("[data-index]")]
        .filter((r) => r.getBoundingClientRect().bottom > viewTop + 1)
        .sort((a, b) => Number(a.dataset.index) - Number(b.dataset.index))[0];
      const top = first ? Number(first.dataset.index) : h.findItemIndex(h.scrollOffset);
      const cut = !!first && first.getBoundingClientRect().top < viewTop - 1;
      const to = promptIndex(lines, dir < 0 && cut ? top + 1 : top, dir);
      if (to < 0) return;
      // Off the bottom, so new rows do not pull the list back to the end.
      atBottom.current = false;
      h.scrollToIndex(to, { align: "start" });
    };
    window.addEventListener(PROMPT_JUMP, onJump);
    return () => window.removeEventListener(PROMPT_JUMP, onJump);
  }, [testid, lines]);

  const jump = () => {
    atBottom.current = true;
    setAway(false);
    handle.current?.scrollToIndex(lines.length - 1, { align: "end" });
  };

  // A send from this chat's composer follows the new message, also when the owner had scrolled up.
  useEffect(() => {
    const onSent = (e: Event) => (e as CustomEvent<string>).detail === openId && lines.length && jump();
    window.addEventListener(SENT, onSent);
    return () => window.removeEventListener(SENT, onSent);
  });

  // Copy buttons inside markdown HTML: one listener for the whole list.
  const onClick = (e: React.MouseEvent) => {
    const layout = (e.target as Element).closest<HTMLElement>("[data-diff-layout]");
    if (layout) return setSplit(layout.dataset.diffLayout === "1");
    const button = (e.target as Element).closest<HTMLElement>("[data-copy]");
    if (!button) return;
    const code = button.closest<HTMLElement>(".md-code");
    if (button.dataset.copy === "code") copyText(code?.dataset.raw ?? code?.querySelector("pre")?.textContent ?? "", "code", button);
    else if (button.dataset.copy === "table") copyText(button.closest<HTMLElement>(".md-table")?.dataset.raw ?? "", "table", button);
  };

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {find.open && <FindBar app={app} view={view} listRef={scrollRef} />}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className="transcript flex-1"
            ref={scrollRef}
            tabIndex={-1}
            data-testid={testid}
            data-chat={openId}
            data-state={chatState}
            onClick={onClick}
          >
            {chatState === "none" &&
              (empty ? (
                <ChatState icon={<MessageSquareDashedIcon className="size-5" />} title="No chat yet">
                  {empty}
                </ChatState>
              ) : (
                <ChatState icon={<MessageSquareDashedIcon className="size-5" />} title="No chat open">
                  Pick a chat in the sidebar, or press <Kbd>⌘K</Kbd> to search.
                </ChatState>
              ))}
            {chatState === "loading" && <ChatSkeleton />}
            {chatState === "error" && (
              <ChatState icon={<TriangleAlertIcon className="size-5 text-failed" />} title="Could not load this chat">
                <span className="font-mono text-xs break-all">{view.chatError}</span>
                <button type="button" className="t-link" onClick={() => void (testid === "transcript" ? (app as App).reading.retryRows() : app.retryRows?.())}>Retry</button>
              </ChatState>
            )}
            {chatState === "ready" && view.md && (
              <>
                <Head app={app} view={view} />
                <Virtualizer
                  key={openId}
                  ref={handle}
                  scrollRef={scrollRef}
                  startMargin={HEAD_HEIGHT}
                  data={lines}
                  shift={view.shift}
                  onScroll={onScroll}
                >
                  {(line: Line, index: number) => (
                    <LineView
                      key={line.id}
                      app={app}
                      line={line}
                      index={index}
                      md={view.md!}
                      full={view.full}
                      // An unreachable agent shows no text box, so its rows offer no Reuse.
                      composerId={view.composer && !view.composer.agent.unreachable ? view.composer.id : undefined}
                      branches={!!view.composer && canBranch(view.composer.agent)}
                      match={line.id === current ? "current" : undefined}
                    />
                  )}
                </Virtualizer>
              </>
            )}
          </div>
        </ContextMenuTrigger>
        <TranscriptMenu lines={lines} listRef={scrollRef} onSource={setSource} />
      </ContextMenu>
      {source && <SourcePopover source={source} listRef={scrollRef} onClose={() => setSource(null)} />}
      {away && chatState === "ready" && (
        <button type="button" className="jump" data-testid="jump-latest" onClick={jump}>
          <ArrowDownIcon className="size-3.5" /> Jump to latest
        </button>
      )}
    </div>
  );
}

/** Top of the loaded history: Load earlier messages. */
function Head({ app, view }: { app: Reader; view: Pane }) {
  return (
    <div className="t-head" style={{ height: HEAD_HEIGHT }}>
      {view.chatError && <span role="alert" className="text-failed">{view.chatError} <button type="button" className="t-link" onClick={() => void app.loadOlder()}>Retry</button></span>}
      {view.hasOlder ? (
        <button type="button" className="t-link" data-testid="load-older" disabled={view.loadingOlder} onClick={() => void app.loadOlder()}>
          {view.loadingOlder ? "Loading earlier messages…" : "Load earlier messages"}
        </button>
      ) : (
        <span className="text-faint-foreground">Start of chat</span>
      )}
    </div>
  );
}

function FindBar({ app, view, listRef }: { app: Reader; view: Pane; listRef: React.RefObject<HTMLDivElement | null> }) {
  const { find } = view;
  const count = find.query.trim() ? (find.matches.length ? `${find.matches.length - find.index} of ${find.matches.length}` : "No matches") : "";
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      app.stepFind(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      app.closeFind();
      focusInputOf(listRef.current);
    }
  };
  return (
    <div className="find" data-testid="find-bar">
      <input
        autoFocus
        data-testid="find-input"
        className="find-input"
        placeholder="Find in loaded messages"
        value={find.query}
        onChange={(e) => app.setFindQuery(e.target.value)}
        onKeyDown={onKeyDown}
      />
      <span className="find-count" data-testid="find-count">
        {count}
      </span>
      {find.query.trim() && !find.matches.length && view.hasOlder && <button type="button" className="t-link" disabled={view.loadingOlder} onClick={() => void app.loadOlder()}>{view.loadingOlder ? "Searching earlier messages…" : "Search earlier messages"}</button>}
      <button type="button" className="icon-btn" aria-label="Previous match" disabled={!find.matches.length} onClick={() => app.stepFind(-1)}>
        <ChevronDownIcon />
      </button>
      <button type="button" className="icon-btn" aria-label="Next match" disabled={!find.matches.length} onClick={() => app.stepFind(1)}>
        <ChevronUpIcon />
      </button>
      <button type="button" className="icon-btn" aria-label="Close find" onClick={() => app.closeFind()}>
        <XIcon />
      </button>
    </div>
  );
}

function ChatState({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div role="status" className="flex h-full flex-col items-center justify-center gap-2 text-center">
      <div className="grid size-10 place-items-center rounded-lg border border-border-strong bg-surface text-muted-foreground">{icon}</div>
      <div className="mt-1 text-base font-medium">{title}</div>
      <div className="max-w-[420px] text-sm text-muted-foreground">{children}</div>
    </div>
  );
}

function ChatSkeleton() {
  return (
    <div aria-label="Loading chat" role="status" className="grid gap-4 pt-6">
      {[78, 92, 64, 86].map((w, i) => (
        <div key={i} className="grid gap-2">
          <div className="h-2.5 rounded-full bg-selected" style={{ width: `${w}%` }} />
          <div className="h-2.5 rounded-full bg-selected" style={{ width: `${w - 24}%` }} />
        </div>
      ))}
    </div>
  );
}
