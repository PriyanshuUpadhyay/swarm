import { useMemo, useState, useSyncExternalStore } from "react";
import type { App } from "../core/app";
import { whenPainted } from "../core/perf";
import { paletteSections, type PaletteItem } from "../core/nav";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "../components/ui/command";
import { Button } from "../components/ui/button";
import { Kbd } from "../components/ui/kbd";
import { chordLabel } from "./chord";
import { StatusGlyph } from "./status";

/** Esc Esc in an empty box: the open chat's messages, newest first; one starts a branch. */
export function BranchPicker({ app }: { app: App }) {
  const open = useSyncExternalStore(app.subscribe, () => app.view.branchPicking);
  // The rows load and grow while the picker is open, so the list follows the chat's row array.
  const loaded = useSyncExternalStore(app.subscribe, () => app.openRows());
  const state = useSyncExternalStore(app.subscribe, () => app.view.chatState);
  const failure = useSyncExternalStore(app.subscribe, () => app.view.chatError);
  const older = useSyncExternalStore(app.subscribe, () => app.view.hasOlder);
  const rows = open ? app.branchRows(loaded) : [];
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
            {state === "loading" && <p role="status" className="px-3 py-4 text-sm text-muted-foreground">Loading messages…</p>}
            {state === "error" && (
              <div role="alert" className="flex items-center gap-2 px-3 py-4 text-sm text-destructive">
                <span className="min-w-0 flex-1">
                  The messages did not load.
                  <span className="block break-words text-xs">{failure}</span>
                </span>
                <Button variant="outline" size="sm" onClick={() => void app.reloadOpenRows()}>
                  Retry
                </Button>
              </div>
            )}
            {(state === "ready" || state === "none") && <CommandEmpty>No messages</CommandEmpty>}
            {rows.map((row) => (
              <CommandItem key={row.id} value={row.id} keywords={[row.text]} onSelect={() => app.branch(row.id)}>
                <span className="truncate">{row.text.replace(/\s+/g, " ")}</span>
              </CommandItem>
            ))}
            {state === "ready" && older && <p className="px-3 py-2 text-xs text-muted-foreground">Older messages are not listed. Scroll up to branch from one.</p>}
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
    </Command>
  );
}
