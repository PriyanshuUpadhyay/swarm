import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { whenPainted } from "../core/perf";
import { paletteSections, type PaletteItem } from "../core/nav";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "../components/ui/command";
import { Button } from "../components/ui/button";
import { Kbd } from "../components/ui/kbd";
import { chordLabel } from "./chord";
import { StatusGlyph } from "./status";

type BranchMessage = { row: string; text: string };
type BranchList = { state: "loading" } | { state: "error"; error: string } | { state: "ready"; messages: BranchMessage[] };

/** Esc Esc in an empty box: every message of the open chat, newest first; one starts a branch.
 *  The server reads the whole chair log, because the page holds only the rows it loaded. */
export function BranchPicker({ app }: { app: App }) {
  const open = useSyncExternalStore(app.subscribe, () => app.view.branchPicking);
  const session = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const [list, setList] = useState<BranchList>({ state: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!open || !session) return;
    let stop = false;
    setList({ state: "loading" });
    app.api
      .get<{ messages: BranchMessage[] }>("/api/chat/branch-messages", { session })
      .then(
        (r) => !stop && setList({ state: "ready", messages: r.messages }),
        (e: unknown) => !stop && setList({ state: "error", error: e instanceof Error ? e.message : String(e) }),
      );
    return () => {
      stop = true;
    };
  }, [app, open, session, attempt]);
  return (
    <CommandDialog
      open={open}
      onOpenChange={(o) => app.setBranchPicker(o)}
      onCloseAutoFocus={(e) => {
        // Esc gives the keys back to the text box, not to the page behind the dialog. A picked row
        // opens the Branch dialog, which keeps the keys until it is gone.
        e.preventDefault();
        whenPainted(
          () => !document.querySelector('[role="dialog"]'),
          () => document.querySelector<HTMLElement>('[data-testid="composer"]')?.focus(),
        );
      }}
      title="Branch from a message"
      description="A new chat starts from the conversation before the message."
    >
      {open && (
        <Command loop label="Branch from a message" data-testid="branch-picker">
          <CommandInput autoFocus placeholder="Branch from which message?" />
          <CommandList>
            {list.state === "loading" && <p role="status" className="px-3 py-4 text-sm text-muted-foreground">Loading messages…</p>}
            {list.state === "error" && (
              <div role="alert" className="flex items-center gap-2 px-3 py-4 text-sm text-destructive">
                <span className="min-w-0 flex-1">
                  The messages did not load.
                  <span className="block break-words text-xs">{list.error}</span>
                </span>
                <Button variant="outline" size="sm" onClick={() => setAttempt((n) => n + 1)}>
                  Retry
                </Button>
              </div>
            )}
            {list.state === "ready" && <CommandEmpty>No messages</CommandEmpty>}
            {list.state === "ready" &&
              list.messages.map((m) => (
                <CommandItem key={m.row} value={m.row} keywords={[m.text]} onSelect={() => app.branch(m.row)}>
                  <span className="truncate">{m.text.replace(/\s+/g, " ")}</span>
                </CommandItem>
              ))}
          </CommandList>
        </Command>
      )}
    </CommandDialog>
  );
}

/** ⌘K. The app owns open state and the items; jean's fuzzy search ranks them (`rankPalette`);
 *  cmdk only draws the list and moves the selection. */
export function Palette({ app }: { app: App }) {
  const open = useSyncExternalStore(app.subscribe, () => app.view.paletteOpen);
  return (
    <CommandDialog open={open} onOpenChange={(o) => app.setPalette(o)}>
      {open && <PaletteBody app={app} />}
    </CommandDialog>
  );
}

function PaletteBody({ app }: { app: App }) {
  // The items change with the tree, the snapshot (through Recent) and the open chat, not with rows.
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const recent = useSyncExternalStore(app.subscribe, () => app.view.recent);
  const openId = useSyncExternalStore(app.subscribe, () => app.view.openId);
  const [query, setQuery] = useState("");
  const items = useMemo(() => app.paletteItems(), [app, tree, recent, openId]);
  const sections = paletteSections(items, query);
  const run = (item: PaletteItem) => {
    if (item.disabledReason) return;
    app.setPalette(false);
    item.run();
  };
  return (
    <Command shouldFilter={false} loop label="Command palette" data-testid="palette">
      <CommandInput autoFocus value={query} onValueChange={setQuery} placeholder="Search actions, workspaces, chats, agents, and prompts" />
      <CommandList>
        <CommandEmpty>No matches</CommandEmpty>
        {sections.map((s) => (
          <CommandGroup key={s.title} heading={s.title}>
            {s.items.map((item) => (
              <CommandItem key={item.id} value={item.id} disabled={!!item.disabledReason} onSelect={() => run(item)} title={item.disabledReason ?? item.title}>
                <span className="grid w-4 shrink-0 place-items-center">{item.status && <StatusGlyph status={item.status} />}</span>
                <span className="truncate">{item.title}</span>
                {item.subtitle && <span className="min-w-0 truncate text-muted-foreground">{item.subtitle}</span>}
                {(item.shortcut || item.disabledReason) && (
                  <CommandShortcut>{item.disabledReason ? item.disabledReason : <Kbd>{chordLabel(item.shortcut!)}</Kbd>}</CommandShortcut>
                )}
              </CommandItem>
            ))}
          </CommandGroup>
        ))}
      </CommandList>
      <details className="border-t border-border px-3 py-2 text-sm" data-testid="keyboard-shortcuts">
        <summary className="cursor-pointer">Keyboard shortcuts</summary>
        <div className="grid max-h-64 gap-1 overflow-y-auto pt-2">
          {app.registry.list().filter((command) => command.chord).map((command) => (
            <div key={command.id} className="flex items-center justify-between gap-3">
              <span>{command.title}</span><Kbd>{chordLabel(command.chord!)}</Kbd>
            </div>
          ))}
        </div>
      </details>
    </Command>
  );
}
