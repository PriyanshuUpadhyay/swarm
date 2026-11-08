import { Fragment, createElement, type ReactNode } from "react";
import type { Block, Display, Inline } from "../core/api";
import { ColoredRow, PatchBody, useSplit } from "./patch-body";

function Head({ label, what, diff = false }: { label: string; what: "code" | "table"; diff?: boolean }) {
  return (
    <div className="md-head">
      <span>{label}</span>
      {diff && (
        <div className="seg" role="group" aria-label="Diff layout">
          <button type="button" data-diff-layout="0">Unified</button>
          <button type="button" data-diff-layout="1">Split</button>
        </div>
      )}
      <button type="button" data-copy={what} aria-label={`Copy ${what}`}>Copy</button>
    </div>
  );
}

function inlines(nodes: Inline[]): ReactNode[] {
  return nodes.map((node, i) => {
    switch (node.type) {
      case "text":
        return <Fragment key={i}>{node.value}</Fragment>;
      case "emphasis":
        return <em key={i}>{inlines(node.value)}</em>;
      case "strong":
        return <strong key={i}>{inlines(node.value)}</strong>;
      case "delete":
        return <s key={i}>{inlines(node.value)}</s>;
      case "code":
        return <code key={i}>{node.value}</code>;
      case "break":
        return <Fragment key={i}><br />{"\n"}</Fragment>;
      case "link": {
        const { url, title, children } = node.value;
        return /^https?:\/\//i.test(url)
          ? <a key={i} href={url} title={title ?? undefined} target="_blank" rel="noopener noreferrer">{inlines(children)}</a>
          : <Fragment key={i}>{inlines(children)}</Fragment>;
      }
      case "later":
        return null;
    }
  });
}

function HunkCode({ label, text, diff }: { label: string; text: string; diff: Extract<NonNullable<Extract<Block, { type: "code" }>["value"]["diff"]>, { lines: unknown }> }) {
  const split = useSplit();
  return (
    <div className="md-code" data-raw={text}>
      <Head label={label} what="code" diff />
      <PatchBody patch={text} info={diff} split={split} />
    </div>
  );
}

function Code({ value: { label, text, diff } }: { value: Extract<Block, { type: "code" }>["value"] }) {
  if (label === null) return <><pre><code>{text}</code></pre>{"\n"}</>;
  if (diff && "lines" in diff) return <HunkCode label={label} text={text} diff={diff} />;
  if (diff && "prefix" in diff) {
    return <div className="md-code"><Head label={label} what="code" /><pre><code>{diff.prefix.map((row, i) => <ColoredRow key={i} kind={row.kind}>{row.text}</ColoredRow>)}</code></pre></div>;
  }
  return (
    <div className="md-code">
      <Head label={label} what="code" />
      <pre><code className={label === "code" ? undefined : `language-${label}`}>{text}</code></pre>{label === "diff" || label === "patch" ? null : "\n"}
    </div>
  );
}

function Table({ value }: { value: Extract<Block, { type: "table" }>["value"] }) {
  const cells = (row: Inline[][], head: boolean) => row.map((cell, i) => {
    const align = value.align[i];
    const style = align && align !== "none" ? { textAlign: align } : undefined;
    return head ? <th key={i} style={style}>{inlines(cell)}</th> : <td key={i} style={style}>{inlines(cell)}</td>;
  });
  return (
    <div className="md-table" data-raw={value.source}>
      <Head label="Table" what="table" />
      <div className="md-scroll">
        <table>
          <thead><tr>{cells(value.head, true)}</tr></thead>
          {value.rows.length > 0 && <tbody>{value.rows.map((row, i) => <tr key={i}>{cells(row, false)}</tr>)}</tbody>}
        </table>
      </div>
    </div>
  );
}

function blocks(nodes: Block[], tight = false): ReactNode[] {
  return nodes.map((node, i) => {
    switch (node.type) {
      case "paragraph": {
        const next = nodes[i + 1]?.type;
        const lineFeed = next === "heading" || next === "thematic_break" || next === "blockquote" || next === "list";
        return tight
          ? <Fragment key={i}>{inlines(node.value)}{lineFeed ? "\n" : null}</Fragment>
          : <Fragment key={i}><p>{inlines(node.value)}</p>{"\n"}</Fragment>;
      }
      case "heading":
        return <Fragment key={i}>{createElement(`h${node.value.depth}`, null, inlines(node.value.children))}{"\n"}</Fragment>;
      case "thematic_break":
        return <Fragment key={i}><hr />{"\n"}</Fragment>;
      case "blockquote":
        return <Fragment key={i}><blockquote>{blocks(node.value)}</blockquote>{"\n"}</Fragment>;
      case "list": {
        const { ordered, start, tight, items } = node.value;
        const children = items.map((item, j) => <Fragment key={j}><li>{blocks(item, tight)}</li>{"\n"}</Fragment>);
        return <Fragment key={i}>{ordered ? <ol start={start !== null && start !== 1 ? start : undefined}>{children}</ol> : <ul>{children}</ul>}{"\n"}</Fragment>;
      }
      case "code":
        return <Code key={i} value={node.value} />;
      case "table":
        return <Table key={i} value={node.value} />;
      case "later":
        return null;
    }
  });
}

export function MarkdownView({ display }: { display: Display }) {
  return <>{blocks(display.blocks)}</>;
}
