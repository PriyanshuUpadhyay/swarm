// Transcript rows, message bubbles, and disclosures.
import { CheckIcon, ChevronDownIcon, ChevronRightIcon, CircleStopIcon, CopyIcon, FileCodeIcon, GitBranchIcon, MailIcon, PencilIcon, XIcon } from "lucide-react";
import { useState } from "react";
import type { Row, TextPreview } from "../core/api";
import type { FullText, Reader } from "../core/app";
import { duration, imagesOf, isCliJoin, LIVE_THOUGHT, paintBodyOf, UNMAPPED, wordAt, type Line } from "../core/transcript";
import { fold, type FoldLevel } from "../core/fold-display";
import { Kbd } from "../components/ui/kbd";
import { copyText } from "./copy";
import { CutLine, plural, sourceLines } from "./cut-line";
import { ImageThumbs } from "./images";
import { cutMarkdown, ImageLoader, MarkdownView } from "./markdown-view";
import { DiffView, OutputBox } from "./transcript-output";
import { PlanRow, TodoRow } from "./transcript-todo";
import { SubagentRow } from "./subagent-chat";
import { ShellRow, StatusGlyph, ToolCard } from "./transcript-tool";

type CutState = { key: string | Row; shown: boolean; reveal: () => void };
type Ctx = { app: Reader; line: Line; full: ReadonlyMap<string, FullText>; cutState?: CutState; composerId?: string; branches?: boolean; match?: string };

function LineView({ index, queueStart, showQueueHint, ...ctx }: Ctx & { index: number; queueStart?: boolean; showQueueHint?: boolean }) {
  const { app, line, match } = ctx;
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
      <ImageLoader.Provider value={app.imageUrl}>
        {queueStart && <div className="flex h-5 items-center justify-end gap-1 text-xs text-muted-foreground">
          {showQueueHint && <><Kbd>↑</Kbd> to edit queued messages</>}
        </div>}
        <RowBody {...ctx} />
      </ImageLoader.Provider>
    </div>
  );
}

const LABEL: Partial<Record<Row["kind"], string>> = {
  thought: "Thinking",
  toolResult: "Result",
  diff: "Changes",
  question: "Question",
  notice: "Notice",
  system: "System",
  result: "Result",
  toolUse: "Tool",
  subagent: "Subagent",
};

function MdBody({ row }: { row: Row }) {
  const body = paintBodyOf(row);
  switch (body.kind) {
    case "plain":
      return <div className="whitespace-pre-wrap">{row.text}</div>;
    case "tree":
      return <div className="md"><MarkdownView display={body.display} /></div>;
  }
}

const CUT_AT = 12;

function MessageCut({ line: { row }, cutState, match, thought }: Pick<Ctx, "line" | "cutState" | "match"> & { thought?: boolean }) {
  const [opened, setOpened] = useState<string | Row | true | null>(null);
  const key = cutState?.key ?? true;

  const body = thought ? null : paintBodyOf(row);
  const open = cutState?.shown || opened === key || !!match;
  const show = () => {
    cutState?.reveal();
    setOpened(key);
  };

  if (body?.kind === "tree") {
    const cut = open ? null : cutMarkdown(body.display, CUT_AT);
    return <>
      <div className="md"><MarkdownView display={cut?.hidden ? cut.display : body.display} /></div>
      {!!cut?.hidden && <CutLine count={`+${plural(cut.hidden, "line")}`} show={show} />}
    </>;
  }

  const lines = open ? [] : sourceLines(row.text);
  const hidden = Math.max(0, lines.length - CUT_AT);
  return <>
    <div className={thought ? "thought" : "whitespace-pre-wrap"} data-testid={thought ? "thought-text" : undefined}>{hidden ? lines.slice(0, CUT_AT).join("\n") : row.text}</div>
    {hidden > 0 && <CutLine count={`+${plural(hidden, "line")}`} show={show} />}
  </>;
}

function isQueued(row: Row): boolean {
  return row.kind === "queued" || row.system_kind === "queued_prompt";
}

// The level each reply is read at, by row id. Rows remount as the transcript scrolls, so a
// component state alone would lose the reader's choice.
const foldLevels = new Map<string, FoldLevel>();
const FOLD_LEVELS: { level: FoldLevel; label: string }[] = [
  { level: "full", label: "Full" },
  { level: "points", label: "Points" },
  { level: "outline", label: "Outline" },
];

/** A long reply with a Full, Points, and Outline switch; a reply that a fold would barely shorten has none. */
function FoldableBody({ row }: { row: Row }) {
  const [level, setLevel] = useState<FoldLevel>(() => foldLevels.get(row.id) ?? "full");
  const body = paintBodyOf(row);
  if (body.kind !== "tree") return <MdBody row={row} />;
  const offered = FOLD_LEVELS.filter(({ level: l }) => fold(body.display, l) !== null);
  if (offered.length < 2) return <MdBody row={row} />;
  const shown = fold(body.display, level) ?? body.display;
  const pick = (next: FoldLevel) => {
    foldLevels.set(row.id, next);
    setLevel(next);
  };
  return (
    <>
      <div className="seg fold-levels" role="group" aria-label="Reply detail">
        {offered.map(({ level: l, label }) => (
          <button key={l} type="button" aria-pressed={l === level} onClick={() => pick(l)}>{label}</button>
        ))}
      </div>
      <div className="md"><MarkdownView display={shown} /></div>
    </>
  );
}

function RowBody(ctx: Ctx) {
  const { app, line } = ctx;
  const { row } = line;
  if (row.kind === "fold") return <FoldLine app={app} line={line} />;
  if (row.kind === "todoList") return <TodoRow row={row} />;
  if (row.kind === "proposedPlan") return <PlanRow row={row} />;
  if (row.subagent) return <SubagentRow run={row.subagent} title={row.text} sourceSession={row.source_session} />;
  if (row.kind === "model_change") return isCliJoin(row)
    ? <div className="divider" data-testid="model-change"><span className="min-w-0 text-center break-words">{row.text}</span></div>
    : <div className="quiet" data-testid="model-change">{row.text}</div>;
  if (row.tool) return <ToolCard {...ctx} />;
  if (row.kind === "divider")
    return (
      <div className="divider">
        <span>{row.text}</span>
        {row.detail && <span className="text-muted-foreground">{row.detail}</span>}
      </div>
    );
  if (row.system_kind === UNMAPPED)
    return (
      <div className="quiet" data-testid="unmapped">
        <Disclosure app={app} line={line} title={row.text}>
          <div className="grid gap-1">
            {(row.steps ?? []).map((step) => step.output && <OutputBox key={step.id} app={app} preview={step.output} full={ctx.full} title="Unmapped record" />)}
          </div>
        </Disclosure>
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
        <MdBody row={row} />
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
        queued={isQueued(row)}
        reuse={row.kind === "user" && composerId ? () => app.reuse(composerId, row.text) : undefined}
        branch={row.kind === "user" && app.branch && (row.source_session ? app.canBranchFrom?.(row.source_session) : ctx.branches) ? () => app.branch?.(row.id) : undefined}
      >
        <MessageCut {...ctx} />
        <ImageThumbs paths={imagesOf(row)} load={app.imageUrl} />
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
        {/* Shown only in annotate mode (ADR 0162); it floats, so the mode never moves a row. */}
        <button type="button" className="whole-reply" data-note-reply aria-label="Add a note on the whole reply">Whole reply</button>
        <FoldableBody row={row} />
      </div>
    );
  const label = <div className="row-label">{LABEL[row.kind] ?? row.kind}</div>;
  switch (row.kind) {
    case "thought":
      if (row.id === LIVE_THOUGHT) return (
        <div className="thought live-thought" data-testid="thought-text">
          <div>{sourceLines(row.text).slice(-4).join("\n")}</div>
        </div>
      );
      // The CLI wrote no summary, so the row only shows that the agent thought.
      if (!row.text.trim()) return <div className="quiet">{LABEL.thought}</div>;
      return (
        <Disclosure app={app} line={line} title={LABEL.thought ?? row.kind}>
          <div className="pl-[18px]">
            <MessageCut {...ctx} thought />
          </div>
        </Disclosure>
      );
    case "diff":
      return row.diff ? <DiffView app={app} diff={row.diff} full={ctx.full} /> : null;
    case "toolResult":
      return (
        <div className="grid gap-1">
          <OutputBox app={app} preview={row.output ?? textPreview(row)} full={ctx.full} title={row.tool_status === "interrupted" ? "Interrupted tool result" : row.tool_status === "failed" ? "Failed tool result" : "Tool result"} />
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
        <div className="cli-error">
          <XIcon className="size-3.5 text-failed" aria-hidden />
          <span className="font-semibold">Error</span>
          <span className="font-mono text-failed whitespace-pre-wrap">{row.text}</span>
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

function Bubble({ row, queued, reuse, branch, children }: { row: Row; queued?: boolean; reuse?: () => void; branch?: () => void; children: React.ReactNode }) {
  return (
    <div className="bubble-row">
      <CopyMessage row={row} />
      {branch && (
        <button type="button" className="copy-msg icon-btn" data-testid="branch-here" aria-label="Branch from here" title="Branch from here: a new chat from before this message" onClick={branch}>
          <GitBranchIcon />
        </button>
      )}
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
    <button type="button" className="copy-msg icon-btn" aria-label="Copy message" title="Copy message" onClick={(e) => copyText(row.text, "message", e.currentTarget)}>
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

export { LineView };
export type { Ctx };
