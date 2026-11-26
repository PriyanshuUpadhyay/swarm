import { ArchiveRestoreIcon, ChevronRightIcon, GitBranchIcon, GitPullRequestIcon, GripVerticalIcon, PinIcon, PlusIcon } from "lucide-react";
import { memo } from "react";
import { shownStatus, type Field, type NavRow, type SidebarRow, type WorkspaceFacts } from "../core/nav";
import { BranchDiff, PrChip } from "./pr-chip";
import { StatusGlyph } from "./status";

/** One sidebar line. Memoized on its wire object, so a key move renders only two rows.
 *  `canNewIn`: a project header whose project has a folder that a new chat can start in. */
export const RowView = memo(function RowView({ row, selected, marked, current, open, archive, canNewIn, facts }: {
  row: NavRow; selected: boolean; marked: boolean | null; current: boolean; open: boolean;
  archive: boolean; canNewIn: boolean; facts: WorkspaceFacts | undefined;
}) {
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
      <div {...common} className={`trow head ${s.kind}`} data-drop={s.kind === "pinned" ? "pin" : undefined} title={s.path ?? undefined}>
        <div className="tline">
          {s.kind === "pinned" ? (
            <>
              <PinIcon className="avatar-icon" />
              <span className="label">Pinned</span>
              {folded && s.status && <StatusGlyph status={s.status} />}
            </>
          ) : s.kind === "needs-you" || s.kind === "more-projects" ? (
            <>
              <span className="label">{s.title}</span>
              {s.kind === "needs-you" && <span className="sidebar-count">{s.count}</span>}
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
          {s.kind === "project" && !archive && canNewIn && (
            <button tabIndex={-1} className="row-btn" data-action="new-chat-project" aria-label={`New chat in ${s.title}`} title="New chat (⌘N)">
              <PlusIcon />
            </button>
          )}
        </div>
      </div>
    );
  }

  if (r.kind === "more") {
    return (
      <div {...common} className="trow sidebar-more">
        <div className="tline">
          <span className="label">{r.title}</span>
          <button type="button" tabIndex={-1} className="fld link">Show all</button>
        </div>
      </div>
    );
  }

  if (r.kind === "chat" && !archive) {
    const needsYou = row.section.kind === "needs-you";
    const other = r.fields.filter((f) => !["title", "unread", "status", "children", "age", "branch", "pr", "ci", "state"].includes(f.field));

    return (
      <div {...common} className="trow chat-row" data-session={r.selection?.chat ?? undefined} data-marked={marked || undefined}
        style={{ paddingLeft: 9 + (row.level - 2) * 11 }}>
        <div className="tline chat-title">
          {needsYou ? <span className="needs-dot" aria-hidden /> : r.hasChildren ? chev() : null}
          <span className="label" title={r.title}>{r.title}</span>
          {r.fields.some((f) => f.field === "unread") && <span className="unread" role="img" aria-label="Unread" data-tip="Unread" />}
          <span className="flex-1" />
          {r.childrenSummary && (
            <button className="sidebar-count" type="button" tabIndex={-1} data-action="toggle">{r.childrenSummary}</button>
          )}
        </div>
        <div className="tline chat-sub">
          {r.status && <><StatusGlyph status={r.status} />{!needsYou && <span className="fld">{shownStatus(r.status)}</span>}</>}
          {needsYou && r.detail && <span className="label" title={r.detail}>{r.detail}</span>}
          {r.age && <FieldView f={{ field: "age", text: r.age }} row={r} />}
          {other.map((f, i) => <FieldView key={i} f={f} row={r} />)}
          <span className="flex-1" />
          <PrChip pr={facts?.pr} tabIndex={-1} />
        </div>
        {open && facts?.branch && (
          <div className="tline chat-sub chat-branch" data-testid="sidebar-branch">
            <GitBranchIcon className="size-3 shrink-0" aria-hidden />
            <span className="label" title={facts.branch}>{facts.branch}</span>
            <BranchDiff diff={facts.diff} />
          </div>
        )}
      </div>
    );
  }

  // Steps can add a second line to other rows.
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
            <button tabIndex={-1} className="row-btn" data-action="new-chat" aria-label={`New chat in ${r.title}`} title="New chat (⌘N)">
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
const CI_STATUS: Record<string, "error" | "working" | "done"> = { "CI failed": "error", "CI pending": "working", "CI passed": "done" };
