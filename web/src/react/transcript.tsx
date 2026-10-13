import { MessageSquareDashedIcon, TriangleAlertIcon } from "lucide-react";
import { useLayoutEffect, useRef, useSyncExternalStore } from "react";
import { Virtualizer, type VirtualizerHandle } from "virtua";
import type { App, View } from "../core/app";
import type { MarkdownCache } from "../core/markdown";
import { inputSummary, previewOf, statusLabel, type Item, type ToolRow } from "../core/transcript";
import { Kbd } from "../components/ui/kbd";

/** The chat column: transcript, its empty, loading and error states, and the composer. */
export function ChatView({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  return (
    <>
      <Transcript app={app} view={view} />
      <div className="shrink-0 px-6 pt-1 pb-4">
        <div className="mx-auto max-w-[820px] rounded-xl border border-border-strong bg-surface focus-within:border-ring">
          <textarea
            className="block min-h-[64px] w-full resize-none bg-transparent px-3 pt-2.5 text-base outline-none placeholder:text-faint-foreground"
            data-testid="composer"
            placeholder="Composer comes in P1"
            readOnly
          />
          <div className="flex items-center gap-2 px-3 pb-2 text-xs text-faint-foreground">
            <span className="flex-1">Read only</span>
            <Kbd>⌘L</Kbd>
          </div>
        </div>
      </div>
    </>
  );
}

function Transcript({ app, view }: { app: App; view: View }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const handle = useRef<VirtualizerHandle>(null);
  const atBottom = useRef(true);
  const { items, openId, chatState } = view;

  // A new chat, or its first page: start at the newest row.
  useLayoutEffect(() => {
    atBottom.current = true;
    if (items.length) handle.current?.scrollToIndex(items.length - 1, { align: "end" });
  }, [openId, chatState]);

  // Appended rows: follow only when the reader is at the bottom.
  useLayoutEffect(() => {
    if (!view.shift && atBottom.current && items.length) handle.current?.scrollToIndex(items.length - 1, { align: "end" });
  }, [items, view.shift]);

  const onScroll = (offset: number) => {
    const h = handle.current;
    if (!h) return;
    atBottom.current = offset + h.viewportSize >= h.scrollSize - 8;
    if (offset < 400) void app.loadOlder();
  };

  return (
    <div className="transcript flex-1" ref={scrollRef} data-testid="transcript" data-chat={openId ?? ""} data-state={chatState}>
      {chatState === "none" && (
        <ChatState icon={<MessageSquareDashedIcon className="size-5" />} title="No chat open">
          Pick a chat in the sidebar, or press <Kbd>⌘K</Kbd> to search.
        </ChatState>
      )}
      {chatState === "loading" && <ChatSkeleton />}
      {chatState === "error" && (
        <ChatState icon={<TriangleAlertIcon className="size-5 text-failed" />} title="Could not load this chat">
          <span className="font-mono text-xs break-all">{view.chatError}</span>
        </ChatState>
      )}
      {chatState === "ready" && view.md && (
        <Virtualizer key={openId} ref={handle} scrollRef={scrollRef} data={items} shift={view.shift} onScroll={onScroll}>
          {(item: Item, index: number) => (
            <ItemView key={item.id} app={app} item={item} index={index} md={view.md!} expanded={view.expanded.has(item.id)} />
          )}
        </Virtualizer>
      )}
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

function ItemView({ app, item, index, md, expanded }: { app: App; item: Item; index: number; md: MarkdownCache; expanded: boolean }) {
  const common = { "data-testid": "row", "data-index": index, "data-id": item.id };
  switch (item.kind) {
    case "agent":
      return <div {...common} className="msg agent" dangerouslySetInnerHTML={{ __html: md.html(item.id, item.text) }} />;
    case "block":
      return (
        <pre {...common} className="block">
          {item.text}
        </pre>
      );
    case "tool":
      return <ToolCard common={common} app={app} row={item} expanded={expanded} />;
    default:
      return (
        <div {...common} className={`msg ${item.kind}`}>
          {item.text}
        </div>
      );
  }
}

function ToolCard({ common, app, row, expanded }: { common: object; app: App; row: ToolRow; expanded: boolean }) {
  const p = row.output === null ? null : previewOf(row);
  const summary = inputSummary(row.input);
  return (
    <div {...common} className={`tool ${row.status}`}>
      <div className="tool-head">
        <b>{row.name || "tool"}</b>
        <span className="status">{statusLabel(row)}</span>
      </div>
      {summary && <pre className="tool-input">{summary}</pre>}
      {p && !expanded && <pre className="tool-out">{p.text}</pre>}
      {p?.truncated && !expanded && <div className="notice">Showing the first 120 lines and 12,000 characters.</div>}
      {p?.truncated && <button onClick={() => app.toggleOutput(row.id)}>{expanded ? "Show less" : "Show full output"}</button>}
      {row.diffs.map((d, i) => (
        <pre key={i} className="diff">
          {[d.path, ...d.hunks.flatMap((h) => h.lines)].slice(0, 201).join("\n")}
        </pre>
      ))}
    </div>
  );
}
