import { Fragment } from "react";
import { isSwarmRow, type CommandList, type Menu, type MenuRow } from "../core/slash";
import { cn } from "../lib/utils";

/** The composer's `/`, `$` and `@` list. The rows arrive with the swarm plugin's rows first. */
export function SlashMenu({ id, menu, rows, current, list, who, select, choose }: {
  id: string;
  menu: Menu;
  rows: MenuRow[];
  current: number;
  list: CommandList | null;
  who: string | null;
  select: (i: number) => void;
  choose: (row: MenuRow, send: boolean) => void;
}) {
  const swarmCount = rows.filter(isSwarmRow).length;
  // Two headings split the groups; one group alone needs none.
  const split = swarmCount > 0 && swarmCount < rows.length;
  return (
    <div
      id={id}
      role="listbox"
      aria-label={menu.opener === "@" ? "Files" : "Commands and skills"}
      data-testid="slash-menu"
      className="absolute right-0 bottom-full left-0 z-20 mb-1 max-h-72 overflow-y-auto rounded-lg border border-border-strong bg-surface p-1 text-sm shadow-lg"
    >
      {rows.map((r, i) => (
        <Fragment key={r.insert}>
          {split && (i === 0 || i === swarmCount) && (
            <div role="presentation" data-testid="slash-group" className={cn("px-2 pt-1 pb-0.5 text-xs text-faint-foreground", i > 0 && "mt-1 border-t border-border pt-2")}>
              {i === 0 ? "Swarm skills" : "Other commands and skills"}
            </div>
          )}
          <div
            id={`${id}-${i}`}
            role="option"
            aria-selected={i === current}
            data-testid="slash-row"
            className={cn("flex cursor-default items-baseline gap-2 rounded-md px-2 py-1", i === current && "bg-hover")}
            onMouseMove={() => select(i)}
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
                title={`Opens a picker in the pane, so Swarm only inserts it. Run \`swarm attach ${who ?? "orchestrator"}\` to use the picker.`}
              >
                opens in pane
              </span>
            )}
            {r.source && <span className="shrink-0 text-xs text-faint-foreground">{r.source}</span>}
          </div>
        </Fragment>
      ))}
      {rows.length === 0 && (
        <div className="px-2 py-1 text-muted-foreground" data-testid="slash-status">
          {list ? "No matches" : menu.opener === "@" ? "Loading files…" : "Loading commands…"}
        </div>
      )}
      {list?.note && <div className="px-2 py-1 text-xs text-failed">{list.note}</div>}
    </div>
  );
}
