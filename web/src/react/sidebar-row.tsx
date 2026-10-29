import { ArchiveRestoreIcon, ChevronRightIcon, GitPullRequestIcon, GripVerticalIcon, PinIcon, PlusIcon } from "lucide-react";
import { memo } from "react";
import { type Field, type NavRow, type RecentChat, type SidebarRow } from "../core/nav";
import { StatusGlyph } from "./status";

/** One sidebar line. Memoized on its wire object, so a key move renders only two rows. */
export const RowView = memo(function RowView({ row, selected, marked, current, archive }: { row: NavRow; selected: boolean; marked: boolean | null; current: boolean; archive: boolean }) {
  const r = row.row;
  const common = {
    role: "treeitem",
    "data-testid": "tree-row",
    "data-kind": row.kind,
    "data-id": row.id,
    "aria-level": row.level,
    "aria-expanded": row.expanded ?? undefined,
    // While chats are picked, the picks are the selection; the keyboard row keeps only the focus.
    "aria-selected": marked ?? selected,
    "aria-current": current || undefined,
    tabIndex: selected ? 0 : -1,
  } as const;
  const chev = (action = true) =>
    row.expanded === null ? <span className="chev" /> : <ChevronRightIcon className="chev" data-open={row.expanded} data-action={action ? "toggle" : undefined} />;

  if (!r) {
    const s = row.section;
    const folded = row.expanded === false;
    return (
      <div {...common} className="trow head" data-drop={s.kind === "pinned" ? "pin" : undefined} title={s.path ?? undefined}>
        <div className="tline">
          {s.kind === "pinned" ? (
            <>
              <PinIcon className="avatar-icon" />
              <span className="label">Pinned</span>
              {folded && s.status && <StatusGlyph status={s.status} />}
            </>
          ) : (
            <>
              <span className="avatar" aria-hidden>
                {s.title.slice(0, 1)}
              </span>
              {s.fields.map((f, i) =>
                f.field === "status" ? folded && s.status ? <StatusGlyph key={i} status={s.status} /> : null : <FieldView key={i} f={f} row={null} />,
              )}
            </>
          )}
          {chev()}
          <span className="flex-1" />
          {s.kind === "project" && !archive && (
            <button tabIndex={-1} className="row-btn" data-action="new-workspace" aria-label={`New workspace in ${s.title}`} title="New workspace (⌘N)">
              <PlusIcon />
            </button>
          )}
        </div>
      </div>
    );
  }

  // Steps, and a chat's children, start a second line (SidebarRowView.lines).
  const lines: Field[][] = [[]];
  for (const f of r.fields) {
    if ((f.field === "steps" || (f.field === "children" && r.kind === "chat")) && lines[lines.length - 1].length) lines.push([]);
    lines[lines.length - 1].push(f);
  }
  if (!r.fields.length) lines[0].push({ field: "title", text: r.title });
  const canNew = r.kind === "workspace" && r.newChatEnabled && !r.archived;
  // A folder's workspace row stands in for the project header, so it takes the header's look.
  const head = row.section.folder && r.kind === "workspace";
  return (
    <div
      {...common}
      className={head ? "trow head" : "trow"}
      data-session={r.kind === "chat" ? (r.selection?.chat ?? undefined) : undefined}
      data-dimmed={r.dimmed || undefined}
      data-marked={marked || undefined}
      draggable={r.kind === "workspace" && !r.archived && !archive ? true : undefined}
      style={head ? undefined : { paddingLeft: 12 + r.depth * 14 }}
    >
      {lines.map((fields, i) => (
        <div key={i} className={i ? "tline sub" : "tline"}>
          {i === 0 && head ? (
            <span className="avatar" aria-hidden>
              {row.section.title.slice(0, 1)}
            </span>
          ) : i === 0 && (r.kind === "workspace" || r.kind === "chat" || r.kind === "archived") && r.hasChildren ? (
            chev()
          ) : (
            <span className="chev" />
          )}
          {fields.map((f, j) => (
            <FieldView key={j} f={f} row={r} />
          ))}
          {i === 0 && head && r.hasChildren && chev()}
          <span className="flex-1" />
          {i === 0 && r.kind === "workspace" && !r.archived && !archive && <GripVerticalIcon className="grip" aria-hidden data-tip="Drag to reorder or pin workspace" />}
          {i === 0 && r.kind === "archived-chat" && (
            <button tabIndex={-1} className="row-btn" data-action="unarchive" aria-label={`Unarchive ${r.title}`} title="Unarchive and open">
              <ArchiveRestoreIcon />
            </button>
          )}
          {i === 0 && canNew && (
            <button tabIndex={-1} className="row-btn" data-action="new-chat" aria-label={`New chat in ${r.title}`} title="New chat (⌘T)">
              <PlusIcon />
            </button>
          )}
        </div>
      ))}
    </div>
  );
});

/** One row field (RowFieldLabel). Hovering a row swaps its age for per-status counts, in CSS. */
function FieldView({ f, row }: { f: Field; row: SidebarRow | null }) {
  if (f.status) return <StatusGlyph status={f.status} />;
  switch (f.field) {
    case "title":
      return <span className="label">{f.text}</span>;
    case "unread":
      return <span className="unread" role="img" aria-label="Unread" data-tip="Unread" />;
    case "children":
      return row?.kind === "chat" && row.hasChildren ? (
        <span className="fld link" data-action="toggle">
          {f.text}
        </span>
      ) : (
        <span className="fld">{f.text}</span>
      );
    case "steps":
      return (
        <span className="fld" data-tip="The Runs view comes later">
          {f.text}
        </span>
      );
    case "age":
      return row?.counts.length ? (
        <>
          <span className="fld age swap">{f.text}</span>
          <span className="fld counts">
            {row.counts.map((c) => (
              <span key={c.status} className="inline-flex items-center gap-0.5">
                <StatusGlyph status={c.status} />
                {c.count}
              </span>
            ))}
          </span>
        </>
      ) : (
        <span className="fld">{f.text}</span>
      );
    case "pr":
      return (
        <span className="fld pr">
          <GitPullRequestIcon className="size-3" aria-hidden />
          {f.text}
        </span>
      );
    case "ci":
      return CI_STATUS[f.text] ? (
        <span className="fld ci">
          <StatusGlyph status={CI_STATUS[f.text]} />
          {f.text}
        </span>
      ) : (
        <span className="fld ci">{f.text}</span>
      );
    default:
      return <span className={`fld ${f.field}`}>{f.text}</span>;
  }
}

/** The row's CI text from Rust (`github_fields`) as a status glyph. */
const CI_STATUS: Record<string, "failed" | "working" | "done"> = { "CI failed": "failed", "CI pending": "working", "CI passed": "done" };

export const RecentRow = memo(function RecentRow({ r, selected, current }: { r: RecentChat; selected: boolean; current: boolean }) {
  return (
    <div
      role="option"
      data-testid="recent-row"
      data-session={r.session}
      aria-selected={selected}
      aria-current={current || undefined}
      tabIndex={selected ? 0 : -1}
      title={r.workspacePath}
      draggable
      className="recent-row"
    >
      <span className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">{r.title}</span>
        {r.unread && <span className="unread" role="img" aria-label="Unread" />}
        {r.status && <StatusGlyph status={r.status} />}
      </span>
      <span className="flex items-center gap-2 text-[11px]">
        <span className="min-w-0 flex-1 truncate">{r.project && r.project !== r.workspace ? `${r.project} · ${r.workspace}` : r.workspace}</span>
        <span className="shrink-0 text-[10px] tabular-nums">{r.age}</span>
      </span>
    </div>
  );
});
