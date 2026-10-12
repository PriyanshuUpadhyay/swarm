import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { Virtualizer, type VirtualizerHandle } from "virtua";
import type { App, View } from "../core/app";
import { boot } from "../core/boot";
import type { MarkdownCache } from "../core/markdown";
import { inputSummary, previewOf, statusLabel, type Item, type ToolRow } from "../core/transcript";
import type { TreeRow } from "../core/tree";
import "../core/style.css";

function Root({ app }: { app: App }) {
  const view = useSyncExternalStore(app.subscribe, () => app.view);
  useEffect(() => app.start(), [app]);
  return (
    <div className="app">
      <Sidebar app={app} rows={view.rows} selected={view.selected} loaded={view.loaded} />
      <div className="main">
        {view.error ? <p className="message">{view.error}</p> : <Transcript app={app} view={view} />}
        <textarea className="composer" data-testid="composer" placeholder="Composer (not part of this pilot)" readOnly />
      </div>
      {view.paletteOpen && <Palette app={app} view={view} />}
    </div>
  );
}

function Sidebar({ app, rows, selected, loaded }: { app: App; rows: TreeRow[]; selected: string | null; loaded: boolean }) {
  return (
    <div className="sidebar" role="tree" aria-label="Chats" data-testid="sidebar" data-loaded={String(loaded)}>
      {rows.map((row) => (
        <TreeRowView key={row.id} app={app} row={row} selected={row.id === selected} />
      ))}
    </div>
  );
}

function TreeRowView({ app, row, selected }: { app: App; row: TreeRow; selected: boolean }) {
  return (
    <div
      className={`trow d${row.depth}`}
      role="treeitem"
      data-testid="tree-row"
      data-kind={row.kind}
      data-id={row.id}
      aria-level={row.depth}
      aria-expanded={row.expanded ?? undefined}
      aria-selected={selected}
      tabIndex={selected ? 0 : -1}
      title={row.title}
      onClick={() => app.clickRow(row.id)}
    >
      <span className="chev">{row.expanded === null ? "" : row.expanded ? "▾" : "▸"}</span>
      <span className="label">{row.label}</span>
      {row.state !== "idle" && <span className={`dot ${row.state}`} aria-label={row.state} />}
    </div>
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
    <div className="transcript" ref={scrollRef} data-testid="transcript" data-chat={openId ?? ""} data-state={chatState}>
      {chatState === "none" && <p className="empty">Select a chat.</p>}
      {chatState === "loading" && <p className="empty">Loading…</p>}
      {chatState === "error" && <p className="empty">{view.chatError}</p>}
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

function ItemView({ app, item, index, md, expanded }: { app: App; item: Item; index: number; md: MarkdownCache; expanded: boolean }) {
  const common = { "data-testid": "row", "data-index": index, "data-id": item.id };
  switch (item.kind) {
    case "agent":
      return <div {...common} className="msg agent" dangerouslySetInnerHTML={{ __html: md.html(item.id, item.text) }} />;
    case "block":
      return <pre {...common} className="block">{item.text}</pre>;
    case "tool":
      return <ToolCard common={common} app={app} row={item} expanded={expanded} />;
    default:
      return <div {...common} className={`msg ${item.kind}`}>{item.text}</div>;
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

function Palette({ app, view }: { app: App; view: View }) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const entries = app.paletteEntries(view.roots, query);
  const close = () => app.setPalette(false);
  const run = (i: number) => {
    const entry = entries[i];
    close();
    entry?.run();
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") setIndex(Math.min(index + 1, entries.length - 1));
    else if (e.key === "ArrowUp") setIndex(Math.max(index - 1, 0));
    else if (e.key === "Enter") run(index);
    else if (e.key === "Escape") close();
    else return;
    e.preventDefault();
  };
  return (
    <div className="palette-backdrop" onClick={close}>
      <div className="palette" data-testid="palette" role="dialog" aria-label="Command palette" onClick={(e) => e.stopPropagation()}>
        <input
          autoFocus
          value={query}
          placeholder="Search commands and chats"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={entries[index] ? `pal-${index}` : undefined}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={onKey}
        />
        <ul id="palette-list" role="listbox">
          {entries.map((entry, i) => (
            <li key={entry.id} id={`pal-${i}`} role="option" aria-selected={i === index} onClick={() => run(i)}>
              <span>{entry.title}</span>
              {entry.hint && <kbd>{entry.hint}</kbd>}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

const app = boot();
const root = createRoot(document.getElementById("root")!);
root.render(typeof app === "string" ? <p className="message">{app}</p> : <Root app={app} />);
