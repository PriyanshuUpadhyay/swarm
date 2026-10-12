import { createEffect, createMemo, createSelector, createSignal, For, on, onCleanup, onMount, Show, type Accessor } from "solid-js";
import { render } from "solid-js/web";
import { Virtualizer, type VirtualizerHandle } from "virtua/solid";
import type { App, View } from "../core/app";
import { boot } from "../core/boot";
import { inputSummary, previewOf, statusLabel, type Item, type ToolRow } from "../core/transcript";
import type { TreeRow } from "../core/tree";
import "../core/style.css";

function Root(props: { app: App }) {
  const app = props.app;
  const [view, setView] = createSignal<View>(app.view, { equals: false });
  onCleanup(app.subscribe(() => setView(app.view)));
  onMount(() => onCleanup(app.start()));
  return (
    <div class="app">
      <Sidebar app={app} view={view} />
      <div class="main">
        <Show when={!view().error} fallback={<p class="message">{view().error}</p>}>
          <Transcript app={app} view={view} />
        </Show>
        <textarea class="composer" data-testid="composer" placeholder="Composer (not part of this pilot)" readOnly />
      </div>
      <Show when={view().paletteOpen}>
        <Palette app={app} view={view} />
      </Show>
    </div>
  );
}

function Sidebar(props: { app: App; view: Accessor<View> }) {
  const isSelected = createSelector(() => props.view().selected);
  return (
    <div class="sidebar" role="tree" aria-label="Chats" data-testid="sidebar" data-loaded={String(props.view().loaded)}>
      <For each={props.view().rows}>{(row) => <TreeRowView app={props.app} row={row} selected={isSelected(row.id)} />}</For>
    </div>
  );
}

function TreeRowView(props: { app: App; row: TreeRow; selected: boolean }) {
  const row = props.row;
  return (
    <div
      class={`trow d${row.depth}`}
      role="treeitem"
      data-testid="tree-row"
      data-kind={row.kind}
      data-id={row.id}
      aria-level={row.depth}
      aria-expanded={row.expanded ?? undefined}
      aria-selected={props.selected}
      tabIndex={props.selected ? 0 : -1}
      title={row.title}
      onClick={() => props.app.clickRow(row.id)}
    >
      <span class="chev">{row.expanded === null ? "" : row.expanded ? "▾" : "▸"}</span>
      <span class="label">{row.label}</span>
      <Show when={row.state !== "idle"}>
        <span class={`dot ${row.state}`} aria-label={row.state} />
      </Show>
    </div>
  );
}

function Transcript(props: { app: App; view: Accessor<View> }) {
  let scrollEl!: HTMLDivElement;
  const view = props.view;
  const readyChat = createMemo(() => (view().chatState === "ready" ? view().openId : null));
  return (
    <div class="transcript" ref={scrollEl} data-testid="transcript" data-chat={view().openId ?? ""} data-state={view().chatState}>
      <Show when={view().chatState === "none"}>
        <p class="empty">Select a chat.</p>
      </Show>
      <Show when={view().chatState === "loading"}>
        <p class="empty">Loading…</p>
      </Show>
      <Show when={view().chatState === "error"}>
        <p class="empty">{view().chatError}</p>
      </Show>
      <Show when={readyChat()} keyed>
        {(_id) => <ChatList app={props.app} view={view} scrollEl={scrollEl} />}
      </Show>
    </div>
  );
}

/** Mounted once per open chat, so virtua's size cache never mixes two chats. */
function ChatList(props: { app: App; view: Accessor<View>; scrollEl: HTMLDivElement }) {
  let handle: VirtualizerHandle | undefined;
  let atBottom = true;
  const view = props.view;
  const items = createMemo(() => view().items);
  const toEnd = () => {
    const n = items().length;
    if (n && handle) handle.scrollToIndex(n - 1, { align: "end" });
  };
  onMount(toEnd);
  createEffect(
    on(items, () => {
      if (!view().shift && atBottom) toEnd();
    }, { defer: true }),
  );
  const onScroll = (offset: number) => {
    if (!handle) return;
    atBottom = offset + handle.viewportSize >= handle.scrollSize - 8;
    if (offset < 400) void props.app.loadOlder();
  };
  return (
    <Virtualizer ref={(h) => (handle = h)} scrollRef={props.scrollEl} data={items()} shift={view().shift} onScroll={onScroll}>
      {(item, index) => <ItemView app={props.app} view={view} item={item} index={index} />}
    </Virtualizer>
  );
}

function ItemView(props: { app: App; view: Accessor<View>; item: Item; index: Accessor<number> }) {
  const item = props.item;
  const common = { "data-testid": "row", "data-id": item.id };
  switch (item.kind) {
    case "agent": {
      // Component bodies run untracked, so the HTML is set once per row object.
      const html = props.view().md!.html(item.id, item.text);
      return <div {...common} data-index={props.index()} class="msg agent" innerHTML={html} />;
    }
    case "block":
      return (
        <pre {...common} data-index={props.index()} class="block">
          {item.text}
        </pre>
      );
    case "tool":
      return <ToolCard app={props.app} view={props.view} row={item} index={props.index} />;
    default:
      return (
        <div {...common} data-index={props.index()} class={`msg ${item.kind}`}>
          {item.text}
        </div>
      );
  }
}

function ToolCard(props: { app: App; view: Accessor<View>; row: ToolRow; index: Accessor<number> }) {
  const row = props.row;
  const p = row.output === null ? null : previewOf(row);
  const summary = inputSummary(row.input);
  const expanded = () => props.view().expanded.has(row.id);
  return (
    <div data-testid="row" data-id={row.id} data-index={props.index()} class={`tool ${row.status}`}>
      <div class="tool-head">
        <b>{row.name || "tool"}</b>
        <span class="status">{statusLabel(row)}</span>
      </div>
      <Show when={summary}>
        <pre class="tool-input">{summary}</pre>
      </Show>
      <Show when={p && !expanded()}>
        <pre class="tool-out">{p!.text}</pre>
      </Show>
      <Show when={p?.truncated}>
        <Show when={!expanded()}>
          <div class="notice">Showing the first 120 lines and 12,000 characters.</div>
        </Show>
        <button onClick={() => props.app.toggleOutput(row.id)}>{expanded() ? "Show less" : "Show full output"}</button>
      </Show>
      <For each={row.diffs}>{(d) => <pre class="diff">{[d.path, ...d.hunks.flatMap((h) => h.lines)].slice(0, 201).join("\n")}</pre>}</For>
    </div>
  );
}

function Palette(props: { app: App; view: Accessor<View> }) {
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const entries = createMemo(() => props.app.paletteEntries(props.view().roots, query()));
  const close = () => props.app.setPalette(false);
  const run = (i: number) => {
    const entry = entries()[i];
    close();
    entry?.run();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") setIndex(Math.min(index() + 1, entries().length - 1));
    else if (e.key === "ArrowUp") setIndex(Math.max(index() - 1, 0));
    else if (e.key === "Enter") run(index());
    else if (e.key === "Escape") close();
    else return;
    e.preventDefault();
  };
  let input!: HTMLInputElement;
  onMount(() => input.focus());
  return (
    <div class="palette-backdrop" onClick={close}>
      <div class="palette" data-testid="palette" role="dialog" aria-label="Command palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={input}
          value={query()}
          placeholder="Search commands and chats"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={entries()[index()] ? `pal-${index()}` : undefined}
          onInput={(e) => {
            setQuery(e.currentTarget.value);
            setIndex(0);
          }}
          onKeyDown={onKey}
        />
        <ul id="palette-list" role="listbox">
          <For each={entries()}>
            {(entry, i) => (
              <li id={`pal-${i()}`} role="option" aria-selected={i() === index()} onClick={() => run(i())}>
                <span>{entry.title}</span>
                <Show when={entry.hint}>
                  <kbd>{entry.hint}</kbd>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </div>
    </div>
  );
}

const app = boot();
const root = document.getElementById("root")!;
render(() => (typeof app === "string" ? <p class="message">{app}</p> : <Root app={app} />), root);
