// Transcript rows, message bubbles, and disclosures.
import { CheckIcon, ChevronDownIcon, ChevronRightIcon, CircleStopIcon, CopyIcon, FileCodeIcon, LoaderCircleIcon, MailIcon, PencilIcon } from "lucide-react";
import type { Row, TextPreview } from "../core/api";
import type { FullText, Reader } from "../core/app";
import type { MarkdownCache } from "../core/markdown";
import { duration, wordAt, type Line } from "../core/transcript";
import { copyText } from "./copy";
import { ImageThumbs } from "./images";
import { DiffView, OutputBox } from "./transcript-output";
import { ShellRow, StatusGlyph, ToolCard } from "./transcript-tool";

type Ctx = { app: Reader; line: Line; md: MarkdownCache; full: ReadonlyMap<string, FullText>; composerId?: string };

function LineView({ app, line, index, md, full, match, composerId }: Ctx & { index: number; match?: string }) {
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
      <RowBody app={app} line={line} md={md} full={full} composerId={composerId} />
    </div>
  );
}

const LABEL: Partial<Record<Row["kind"], string>> = {
  thought: "Thinking",
  toolResult: "Result",
  diff: "Changes",
  question: "Question",
  error: "Error",
  notice: "Notice",
  system: "System",
  result: "Result",
  toolUse: "Tool",
};

function RowBody(ctx: Ctx) {
  const { app, line, md } = ctx;
  const { row } = line;
  if (row.kind === "fold") return <FoldLine app={app} line={line} />;
  if (row.tool) return <ToolCard {...ctx} />;
  if (row.collapsed) return <Collapsed {...ctx} />;
  if (row.kind === "divider")
    return (
      <div className="divider">
        <span>{row.text}</span>
        {row.detail && <span className="text-faint-foreground">{row.detail}</span>}
      </div>
    );
  if (row.system_kind === "working")
    return (
      <div className="quiet" role="status" data-testid="working">
        <LoaderCircleIcon className="glyph working" /> {row.text}
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
        <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text) }} />
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
        queued={row.kind === "queued" || row.system_kind === "queued_prompt"}
        reuse={row.kind === "user" && composerId ? () => app.reuse(composerId, row.text) : undefined}
      >
        <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text, true) }} />
        <ImageThumbs text={row.text} load={app.imageUrl} />
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
        <div className="md" dangerouslySetInnerHTML={{ __html: md.html(row.id, row.text) }} />
      </div>
    );
  const label = <div className="row-label">{LABEL[row.kind] ?? row.kind}</div>;
  switch (row.kind) {
    case "thought":
      return (
        <Disclosure app={app} line={line} title="Thinking">
          <pre className="plain">{row.text}</pre>
        </Disclosure>
      );
    case "diff":
      return row.diff ? <DiffView app={app} diff={row.diff} full={ctx.full} /> : null;
    case "toolResult":
      return (
        <div className="grid gap-1">
          <OutputBox app={app} preview={row.output ?? textPreview(row)} full={ctx.full} title={row.tool_status === "failed" ? "Failed tool result" : "Tool result"} />
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
        <div className="grid gap-0.5">
          {label}
          <div className="text-failed whitespace-pre-wrap">{row.text}</div>
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

function Bubble({ row, queued, reuse, children }: { row: Row; queued?: boolean; reuse?: () => void; children: React.ReactNode }) {
  return (
    <div className="bubble-row">
      <CopyMessage row={row} />
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
    <button type="button" className="copy-msg icon-btn" aria-label="Copy message" title="Copy message" onClick={() => copyText(row.text, "message")}>
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

/** A collapsed row: one closed line, which opens to the whole row. */
function Collapsed({ app, line, full }: Ctx) {
  const { row } = line;
  const text = row.text.trim();
  const title = text.split("\n", 1)[0] || row.system_kind || row.kind;
  if (!row.output?.text && !text.includes("\n"))
    return <div className="quiet">{title}</div>;
  return (
    <Disclosure app={app} line={line} title={title} mono>
      <OutputBox app={app} preview={row.output ?? textPreview(row)} full={full} title={row.system_kind === "record" ? "Record" : "Text"} />
    </Disclosure>
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
