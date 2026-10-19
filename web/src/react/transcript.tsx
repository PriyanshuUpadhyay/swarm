import {
  ArrowDownIcon,
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  ChevronUpIcon,
  CircleHelpIcon,
  CircleIcon,
  CircleStopIcon,
  CopyIcon,
  FileCodeIcon,
  LoaderCircleIcon,
  MailIcon,
  MessageSquareDashedIcon,
  PencilIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { VList, Virtualizer, type VirtualizerHandle } from "virtua";
import type { DiffPreview, Row, TextPreview, ToolState } from "../core/api";
import { PROMPT_JUMP, type App, type FullText, type Pane, type Reader } from "../core/app";
import type { MarkdownCache } from "../core/markdown";
import { duration, isPatch, parsePatch, patchKinds, promptIndex, splitPatch, wordAt, type Line, type PatchLine, type SplitRow } from "../core/transcript";
import { ContextMenu, ContextMenuTrigger } from "../components/ui/context-menu";
import { Kbd } from "../components/ui/kbd";
import { ChatInput } from "./composer";
import { copyText } from "./copy";
import { ImageThumbs } from "./images";
import { SourcePopover, TranscriptMenu, type Source } from "./transcript-overlays";
import { store } from "./store";

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
    atBottom.current = true;
    setAway(false);
    if (lines.length) handle.current?.scrollToIndex(lines.length - 1, { align: "end" });
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
  }, [find.seq]);

  const onScroll = (offset: number) => {
    const h = handle.current;
    if (!h) return;
    atBottom.current = offset + h.viewportSize >= h.scrollSize - 8;
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

  // Copy buttons inside markdown HTML: one listener for the whole list.
  const onClick = (e: React.MouseEvent) => {
    const layout = (e.target as Element).closest<HTMLElement>("[data-diff-layout]");
    if (layout) return setSplit(layout.dataset.diffLayout === "1");
    const button = (e.target as Element).closest<HTMLElement>("[data-copy]");
    if (!button) return;
    const code = button.closest<HTMLElement>(".md-code");
    if (button.dataset.copy === "code") copyText(code?.dataset.raw ?? code?.querySelector("pre")?.textContent ?? "", "code");
    else if (button.dataset.copy === "table") copyText(button.closest<HTMLElement>(".md-table")?.dataset.raw ?? "", "table");
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
      listRef.current?.focus();
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

type Ctx = { app: Reader; line: Line; md: MarkdownCache; full: ReadonlyMap<string, FullText>; composerId?: string };

function LineView({ app, line, index, md, full, match, composerId }: Ctx & { index: number; match?: string }) {
  const { row } = line;
  return (
    <div
      data-testid="row"
      data-index={index}
      data-id={line.id}
      data-kind={row.kind}
      data-match={match}
      className={`line${line.fold ? " step" : ""}`}
    >
      <RowBody app={app} line={line} md={md} full={full} composerId={composerId} />
    </div>
  );
}

const LABEL: Partial<Record<Row["kind"], string>> = {
  thought: "Thinking",
  toolResult: "Result",
  diff: "Changes",
  question: "Question",
  error: "Error",
  notice: "Notice",
  system: "System",
  result: "Result",
  toolUse: "Tool",
};

function RowBody(ctx: Ctx) {
  const { app, line, md } = ctx;
  const { row } = line;
  if (row.kind === "fold") return <FoldLine app={app} line={line} />;
  if (row.tool) return <ToolCard {...ctx} />;
  if (row.collapsed) return <Collapsed {...ctx} />;
  if (row.kind === "divider")
    return (
      <div className="divider">
        <span>{row.text}</span>
        {row.detail && <span className="text-faint-foreground">{row.detail}</span>}
      </div>
    );
  if (row.system_kind === "working")
    return (
      <div className="quiet" role="status" data-testid="working">
        <LoaderCircleIcon className="glyph working" /> {row.text}
      </div>
    );
  if (row.system_kind === "swarm_ring")
    return (
      <div className="quiet">
        <MailIcon className="size-3.5" /> New swarm message
      </div>
    );
  // Every provider marks the cut as kind compaction; pi's row holds the whole summary, and Claude
  // sends its summary as a separate compact_summary row.
  if (row.system_kind === "compaction" || row.system_kind === "compact_summary")
    return row.text.includes("\n") ? (
      <Disclosure app={app} line={line} title={row.system_kind === "compaction" ? "Context compacted" : "Compaction summary"}>
        <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text) }} />
      </Disclosure>
    ) : (
      <div className="quiet">Context compacted</div>
    );
  if (row.shell) return <ShellRow {...ctx} />;
  if (row.command)
    return (
      <Bubble row={row}>
        <CommandChip row={row} />
      </Bubble>
    );
  if (row.kind === "user" || row.kind === "queued") {
    const composerId = ctx.composerId;
    return (
      <Bubble
        row={row}
        queued={row.kind === "queued" || row.system_kind === "queued_prompt"}
        reuse={row.kind === "user" && composerId ? () => app.reuse(composerId, row.text) : undefined}
      >
        <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text, true) }} />
        <ImageThumbs text={row.text} load={app.imageUrl} />
      </Bubble>
    );
  }
  if (row.ends_turn) {
    const interrupted = row.kind === "notice" || row.text === "aborted";
    const title = row.kind === "notice" ? "Interrupted by you" : interrupted ? "Turn interrupted" : "Turn finished";
    return (
      <div className="quiet">
        {interrupted ? <CircleStopIcon className="size-3.5" /> : <CheckIcon className="size-3.5" />}
        {[title, row.detail].filter(Boolean).join(" · ")}
      </div>
    );
  }
  if (row.kind === "assistant")
    return (
      <div className="msg">
        <CopyMessage row={row} />
        <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text) }} />
      </div>
    );
  const label = <div className="row-label">{LABEL[row.kind] ?? row.kind}</div>;
  switch (row.kind) {
    case "thought":
      return (
        <Disclosure app={app} line={line} title="Thinking">
          <pre className="plain">{row.text}</pre>
        </Disclosure>
      );
    case "diff":
      return row.diff ? <DiffView app={app} diff={row.diff} full={ctx.full} /> : null;
    case "toolResult":
      return (
        <div className="grid gap-1">
          <OutputBox app={app} preview={row.output ?? textPreview(row)} full={ctx.full} title={row.tool_status === "failed" ? "Failed tool result" : "Tool result"} />
        </div>
      );
    case "toolUse":
      return (
        <Disclosure app={app} line={line} title={row.text} mono>
          <pre className="plain">{row.detail}</pre>
        </Disclosure>
      );
    case "error":
      return (
        <div className="grid gap-0.5">
          {label}
          <div className="text-failed whitespace-pre-wrap">{row.text}</div>
        </div>
      );
    case "system":
      if (row.text.split("\n", 4).length > 3)
        return (
          <Disclosure app={app} line={line} title={row.text.slice(0, row.text.indexOf("\n"))} mono>
            <pre className="plain">{row.output?.text ?? row.text}</pre>
          </Disclosure>
        );
    // falls through
    default:
      return (
        <div className="grid gap-0.5">
          {label}
          <div className="whitespace-pre-wrap text-muted-foreground">{row.text}</div>
        </div>
      );
  }
}

/** A row with only text: its own preview, so Show full output still works without a server id. */
const textPreview = (row: Row): TextPreview => ({ id: "", text: row.text, full_lines: 0, full_bytes: 0, hidden_lines: 0 });

function Bubble({ row, queued, reuse, children }: { row: Row; queued?: boolean; reuse?: () => void; children: React.ReactNode }) {
  return (
    <div className="bubble-row">
      <CopyMessage row={row} />
      {reuse && (
        <button
          type="button"
          className="copy-msg icon-btn"
          data-testid="reuse-prompt"
          aria-label="Reuse this prompt"
          title="Reuse this prompt"
          onClick={(e) => {
            reuse();
            // The composer under this transcript, in the chat or in a child's column.
            let box: HTMLElement | null = null;
            for (let el = e.currentTarget.parentElement; el && !box; el = el.parentElement) box = el.querySelector<HTMLElement>('[data-testid="composer"]');
            box?.focus();
          }}
        >
          <PencilIcon />
        </button>
      )}
      <div className={`bubble${queued ? " queued" : ""}`}>
        {queued && <div className="row-label">{row.detail || "Queued"}</div>}
        {children}
      </div>
    </div>
  );
}

function CopyMessage({ row }: { row: Row }) {
  return (
    <button type="button" className="copy-msg icon-btn" aria-label="Copy message" title="Copy message" onClick={() => copyText(row.text, "message")}>
      <CopyIcon />
    </button>
  );
}

/** A command and its words. A skill named mid-message shows its chip where the owner typed it. */
function CommandChip({ row }: { row: Row }) {
  const chip = row.command!;
  const name = (
    <span className="chip">
      <FileCodeIcon className="size-3" /> {chip.name}
    </span>
  );
  const at = wordAt(chip.arguments, chip.name);
  return (
    <div className="grid gap-1">
      {at >= 0 ? (
        <div className="whitespace-pre-wrap">
          {chip.arguments.slice(0, at)}
          {name}
          {chip.arguments.slice(at + chip.name.length)}
        </div>
      ) : (
        <div className="flex items-baseline gap-2">
          {name}
          {chip.arguments && <span className="whitespace-pre-wrap">{chip.arguments}</span>}
        </div>
      )}
      {chip.skill_body && (
        <details className="text-xs">
          <summary className="cursor-default text-muted-foreground">Skill · {chip.name.replace(/^\/(skill:)?/, "")}</summary>
          <pre className="plain">{chip.skill_body.text}</pre>
        </details>
      )}
      {chip.output?.text && <div className="truncate text-xs text-muted-foreground">{chip.output.text}</div>}
    </div>
  );
}

/** A collapsed row: one closed line, which opens to the whole row. */
function Collapsed({ app, line, full }: Ctx) {
  const { row } = line;
  const text = row.text.trim();
  const title = text.split("\n", 1)[0] || row.system_kind || row.kind;
  if (!row.output?.text && !text.includes("\n"))
    return <div className="quiet">{title}</div>;
  return (
    <Disclosure app={app} line={line} title={title} mono>
      <OutputBox app={app} preview={row.output ?? textPreview(row)} full={full} title={row.system_kind === "record" ? "Record" : "Text"} />
    </Disclosure>
  );
}

function Disclosure({ app, line, title, mono, children }: { app: Reader; line: Line; title: string; mono?: boolean; children: React.ReactNode }) {
  return (
    <div className="grid gap-1">
      <button type="button" className="disclosure" aria-expanded={line.open} onClick={() => app.toggleLine(line)}>
        {line.open ? <ChevronDownIcon className="size-3" /> : <ChevronRightIcon className="size-3" />}
        <span className={`truncate${mono ? " font-mono" : ""}`}>{title}</span>
      </button>
      {line.open && children}
    </div>
  );
}

export function StatusGlyph({ state }: { state: ToolState }) {
  const label = STATE_LABEL[state];
  const cls = "glyph-slot";
  switch (state) {
    case "waiting":
      return <CircleIcon aria-label={label} className={`${cls} fill-current text-working`} />;
    case "finished":
      return <CheckIcon aria-label={label} className={`${cls} text-done`} />;
    case "failed":
      return <XIcon aria-label={label} className={`${cls} text-failed`} />;
    case "interrupted":
      return <CircleStopIcon aria-label={label} className={`${cls} text-waiting`} />;
    default:
      return <CircleHelpIcon aria-label={label} className={`${cls} text-muted-foreground`} />;
  }
}

const STATE_LABEL: Record<ToolState, string> = {
  waiting: "Waiting for result",
  finished: "Finished",
  failed: "Failed",
  interrupted: "Interrupted",
  unreported: "No result",
};

/** One line per fold (ADR 0047): its text changes as steps land, its height does not. */
function FoldLine({ app, line }: { app: Reader; line: Line }) {
  const f = line.row.fold;
  return (
    <button type="button" className="fold" data-testid="fold" aria-expanded={line.open} onClick={() => app.toggleLine(line)}>
      {line.open ? <ChevronDownIcon className="glyph-slot text-muted-foreground" /> : <ChevronRightIcon className="glyph-slot text-muted-foreground" />}
      {f && <StatusGlyph state={f.state} />}
      {/* The server's text already names the failed count; the glyph shows it in colour. */}
      <span className="shrink-0 font-semibold">{f?.text ?? line.row.text}</span>
      {f?.is_running && f.latest_title && <span className="truncate text-muted-foreground">· now: {f.latest_title}</span>}
      <span className="flex-1" />
      {f?.duration_seconds !== undefined && <span className="num text-xs text-muted-foreground">{duration(f.duration_seconds)}</span>}
    </button>
  );
}

function ToolCard({ app, line, full }: Ctx) {
  const t = line.row.tool!;
  const counts = t.diffs?.length ? t.diffs.reduce((a, d) => [a[0] + d.added, a[1] + d.removed], [0, 0]) : null;
  const result = [t.exit_code !== undefined ? `exit ${t.exit_code}` : "", t.duration_seconds !== undefined ? duration(t.duration_seconds) : ""].filter(Boolean).join(" · ");
  return (
    <div className={`card${line.open ? " open" : ""}`} data-testid="tool-card" data-state={t.state}>
      <button type="button" className="card-head" aria-expanded={line.open} onClick={() => app.toggleLine(line)}>
        <StatusGlyph state={t.state} />
        <span className="shrink-0 font-semibold">{t.name}</span>
        <span className="min-w-0 flex-1 truncate text-left text-muted-foreground">{t.header_title}</span>
        {counts && (
          <span className="font-mono text-xs">
            <span className="text-done">+{counts[0]}</span> <span className="text-failed">−{counts[1]}</span>
          </span>
        )}
        {result && <span className={`num text-xs ${t.state === "failed" ? "text-failed" : "text-muted-foreground"}`}>{result}</span>}
      </button>
      {t.skill_body && (
        <details className="card-indent text-xs">
          <summary className="cursor-default text-muted-foreground">Body</summary>
          <pre className="plain">{t.skill_body.text}</pre>
        </details>
      )}
      {line.open && (
        <div className="card-body card-indent">
          {t.command && <pre className="plain">{t.command}</pre>}
          {t.path && <div className="font-mono text-xs break-all">{t.path}</div>}
          {t.output ? (
            <OutputBox app={app} preview={t.output} full={full} title="Output" />
          ) : (
            <div className="text-muted-foreground">{t.state === "waiting" ? "Waiting for tool result." : "No tool result was recorded."}</div>
          )}
          {t.diffs?.map((d) => <DiffView key={d.id} app={app} diff={d} full={full} />)}
        </div>
      )}
    </div>
  );
}

function ShellRow({ app, line, full }: Ctx) {
  const run = line.row.shell!;
  const note = run.exit_code !== undefined ? `exit ${run.exit_code}` : !run.output.text ? "no output" : `${plural(run.output.full_lines || 1, "line")}`;
  return (
    <div className="grid gap-1">
      {run.command && (
        <div className="flex items-center gap-2">
          {run.exit_code !== undefined ? <StatusGlyph state={run.exit_code === 0 ? "finished" : "failed"} /> : <span className="glyph-slot font-mono text-muted-foreground">$</span>}
          <span className="min-w-0 flex-1 font-mono break-all">{run.command}</span>
          <span className={`num text-xs ${run.exit_code === undefined ? "text-muted-foreground" : run.exit_code === 0 ? "text-done" : "text-failed"}`}>{note}</span>
        </div>
      )}
      {run.output.text && (
        <div className="card-indent">
          <OutputBox app={app} preview={run.output} full={full} title="Output" />
        </div>
      )}
    </div>
  );
}

const plural = (n: number, word: string) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;

/** The capped preview; Show full output fetches the whole text and shows it in a windowed box. */
function OutputBox({ app, preview, full, title }: { app: Reader; preview: TextPreview; full: ReadonlyMap<string, FullText>; title: string }) {
  const f = preview.id ? full.get(preview.id) : undefined;
  const copy = () => copyText(preview.is_truncated ? app.fullText(preview.id) : preview.text, title.toLowerCase());
  return (
    <div className="grid gap-1" data-testid="output">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <span className="font-medium">{f?.state === "ready" ? `${title} · ${plural(f.lines.length, "line")}` : title}</span>
        <span className="flex-1" />
        <button type="button" className="t-link" onClick={copy}>
          <CopyIcon className="size-3" /> Copy {title.toLowerCase()}
        </button>
      </div>
      {f?.state === "ready" ? (
        <TextLines lines={f.lines} testid="full-output" />
      ) : isPatch(preview.text) ? (
        <PatchText text={preview.text} />
      ) : (
        <pre className="out">{preview.text || "No output was recorded."}</pre>
      )}
      {preview.is_truncated && f?.state !== "ready" && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>… +{plural(preview.hidden_lines, "line")}</span>
          <button type="button" className="t-link" data-testid="show-full" disabled={f?.state === "loading"} onClick={() => void app.loadFull(preview.id)}>
            {f?.state === "loading" ? "Loading full output…" : "Show full output"}
          </button>
          {f?.state === "error" && <span className="text-failed">{f.error}</span>}
        </div>
      )}
    </div>
  );
}

/** Text that holds a unified diff, such as `diff -u` output, with each line in its diff colour. */
function PatchText({ text }: { text: string }) {
  const lines = text.split("\n");
  const kinds = patchKinds(lines);
  return (
    <pre className="out">
      {lines.map((l, i) => (
        <span key={i} className={`dl ${kinds[i]}`}>
          {l}
          {"\n"}
        </span>
      ))}
    </pre>
  );
}

const LINE_HEIGHT = 17;
const BOX_HEIGHT = 360;

/** A windowed box of text lines, so a 200,000-line output keeps a few dozen nodes. */
function TextLines({ lines, testid }: { lines: string[]; testid?: string }) {
  const kinds = useMemo(() => (lines.some((l) => l.startsWith("@@ ")) ? patchKinds(lines) : null), [lines]);
  return (
    <VList className="out vbox" data-testid={testid} style={{ height: Math.min(BOX_HEIGHT, lines.length * LINE_HEIGHT + 12) }} itemSize={LINE_HEIGHT} data={lines}>
      {(text: string, i: number) => (
        <div key={i} className={kinds ? `vline dl ${kinds[i]}` : "vline"}>
          {text || " "}
        </div>
      )}
    </VList>
  );
}

// One Unified/Split choice for every diff, kept across sessions. The markdown diff fences are
// static HTML, so CSS reads the choice from `data-diff-split` on <html>.
const SPLIT_KEY = "swarm.diff.split";
const splitSubs = new Set<() => void>();
let splitNow = store.getItem(SPLIT_KEY) === "1";
document.documentElement.dataset.diffSplit = splitNow ? "1" : "0";

function setSplit(v: boolean) {
  splitNow = v;
  store.setItem(SPLIT_KEY, v ? "1" : "0");
  document.documentElement.dataset.diffSplit = v ? "1" : "0";
  splitSubs.forEach((f) => f());
}

const subscribeSplit = (f: () => void) => {
  splitSubs.add(f);
  return () => splitSubs.delete(f);
};

/** A patch: Unified or Split, the capped preview, Show full patch, Copy patch, and its counts. */
function DiffView({ app, diff, full }: { app: Reader; diff: DiffPreview; full: ReadonlyMap<string, FullText> }) {
  const split = useSyncExternalStore(subscribeSplit, () => splitNow);
  const f = full.get(diff.id);
  const patch = f?.state === "ready" ? f.text : diff.patch;
  const capped = diff.omitted_lines > 0 || diff.shortened_lines > 0;
  const name = diff.path.slice(diff.path.lastIndexOf("/") + 1);
  return (
    <div className="diff" data-testid="diff" data-split={split}>
      <div className="diff-head">
        <span className="font-medium" title={diff.path}>
          {name}
        </span>
        <span className="font-mono text-xs">
          <span className="text-done">+{diff.added}</span> <span className="text-failed">−{diff.removed}</span>
        </span>
        <span className="flex-1" />
        <div className="seg" role="group" aria-label="Diff layout">
          <button type="button" aria-pressed={!split} onClick={() => setSplit(false)}>
            Unified
          </button>
          <button type="button" aria-pressed={split} onClick={() => setSplit(true)}>
            Split
          </button>
        </div>
        <button type="button" className="t-link" onClick={() => copyText(capped ? app.fullText(diff.id) : diff.patch, "patch")}>
          <CopyIcon className="size-3" /> Copy patch
        </button>
      </div>
      <PatchBody patch={patch} split={split} />
      {capped && f?.state !== "ready" && (
        <div className="flex items-center gap-2 px-2 pb-1.5 text-xs text-muted-foreground">
          <span>
            Showing a preview{diff.omitted_lines ? `, ${plural(diff.omitted_lines, "line")} left out` : ""}
            {diff.shortened_lines ? `, ${plural(diff.shortened_lines, "long line")} cut` : ""}.
          </span>
          <button type="button" className="t-link" disabled={f?.state === "loading"} onClick={() => void app.loadFull(diff.id)}>
            {f?.state === "loading" ? "Loading full patch…" : "Show full patch"}
          </button>
          {f?.state === "error" && <span className="text-failed">{f.error}</span>}
        </div>
      )}
    </div>
  );
}

function PatchBody({ patch, split }: { patch: string; split: boolean }) {
  const parsed = useMemo(() => parsePatch(patch), [patch]);
  const rows: (PatchLine | SplitRow)[] = split ? splitPatch(parsed) : parsed.filter((l) => l.kind !== "meta");
  const height = Math.min(BOX_HEIGHT, rows.length * LINE_HEIGHT + 4);
  return (
    <VList className="patch" style={{ height }} itemSize={LINE_HEIGHT} data={rows}>
      {(r: PatchLine | SplitRow, i: number) => {
        if ("hunk" in r) return <div key={i} className="pl hunk">{r.hunk}</div>;
        if ("kind" in r)
          return (
            <div key={i} className={`pl ${r.kind}`}>
              <span className="ln">{r.old ?? ""}</span>
              <span className="ln">{r.new ?? ""}</span>
              <span className="mark">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : r.kind === "hunk" ? "" : " "}</span>
              <span className="code">{r.text}</span>
            </div>
          );
        return (
          <div key={i} className="pl split">
            <Half line={r.left} side="old" />
            <Half line={r.right} side="new" />
          </div>
        );
      }}
    </VList>
  );
}

function Half({ line, side }: { line: PatchLine | null; side: "old" | "new" }) {
  if (!line) return <span className="half empty" />;
  const kind = line.kind === "ctx" ? "ctx" : line.kind;
  return (
    <span className={`half ${kind}`}>
      <span className="ln">{side === "old" ? line.old : line.new}</span>
      <span className="code">{line.text}</span>
    </span>
  );
}
