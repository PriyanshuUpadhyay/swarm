import { CommandIcon, MessageSquareIcon } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
import type { App, PaletteEntry } from "../core/app";
import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "../components/ui/command";
import { Kbd } from "../components/ui/kbd";
import { chordLabel } from "./chord";

/** ⌘K. The app owns open state and search; cmdk only draws the list and moves the selection. */
export function Palette({ app }: { app: App }) {
  const open = useSyncExternalStore(app.subscribe, () => app.view.paletteOpen);
  return (
    <CommandDialog open={open} onOpenChange={(o) => app.setPalette(o)}>
      {open && <PaletteBody app={app} />}
    </CommandDialog>
  );
}

function PaletteBody({ app }: { app: App }) {
  const roots = useSyncExternalStore(app.subscribe, () => app.view.roots);
  const [query, setQuery] = useState("");
  const entries = app.paletteEntries(roots, query);
  // Chat entries carry tree ids ("c:<session>"); everything else is a registered command.
  const commands = entries.filter((e) => !e.id.startsWith("c:"));
  const chats = entries.filter((e) => e.id.startsWith("c:"));
  const run = (entry: PaletteEntry) => {
    app.setPalette(false);
    entry.run();
  };
  return (
    <Command shouldFilter={false} loop label="Command palette" data-testid="palette">
      <CommandInput autoFocus value={query} onValueChange={setQuery} placeholder="Search commands and chats" />
      <CommandList>
        <CommandEmpty>Nothing matches “{query}”.</CommandEmpty>
        {commands.length > 0 && (
          <CommandGroup heading="Commands">
            {commands.map((e) => (
              <CommandItem key={e.id} value={e.id} onSelect={() => run(e)}>
                <CommandIcon />
                {e.title}
                {e.hint && (
                  <CommandShortcut>
                    <Kbd>{chordLabel(e.hint)}</Kbd>
                  </CommandShortcut>
                )}
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        {chats.length > 0 && (
          <CommandGroup heading="Chats">
            {chats.map((e) => (
              <CommandItem key={e.id} value={e.id} onSelect={() => run(e)}>
                <MessageSquareIcon />
                <span className="truncate">{e.title}</span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
      </CommandList>
    </Command>
  );
}
