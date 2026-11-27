import { Fragment, createContext, createElement, useContext, useEffect, useState, type CSSProperties, type ReactNode } from "react";
import type { Block, Display, Inline } from "../core/api";
import { mapCutContainer } from "../core/transcript";
import { HighlightedCode } from "./code-tokens";
import { plural, sourceLines } from "./cut-line";
import { ColoredRow, PatchBody, useSplit, useWrap } from "./patch-body";

const SCROLL_WINDOW_LINES = 20;
const scrollWindowStyle = { "--scroll-window-lines": SCROLL_WINDOW_LINES } as CSSProperties;

function Head({ label, what, diff = false, count = 0 }: { label: string; what: "code" | "table"; diff?: boolean; count?: number }) {
  return (
    <div className="md-head">
      <span>{label}{count > SCROLL_WINDOW_LINES ? ` · ${plural(count, "line")}` : ""}</span>
      {diff && (
        <div className="seg" role="group" aria-label="Diff layout">
          <button type="button" data-diff-layout="0">Unified</button>
          <button type="button" data-diff-layout="1">Split</button>
        </div>
      )}
      {diff && <div className="seg md-wrap"><button type="button" data-wrap-toggle>Wrap</button></div>}
      <button type="button" data-copy={what} aria-label={`Copy ${what}`}>Copy</button>
    </div>
  );
}

/** Loads a local image as a blob URL. Without one, an image shows as its alt text. */
export const ImageLoader = createContext<((path: string) => Promise<string>) | null>(null);

function Image({ alt, path }: { alt: string; path: string }) {
  const load = useContext(ImageLoader);
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!load) return;
    let live = true;
    load(path).then((u) => live && setUrl(u), () => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [load, path]);
  if (url) return <img className="md-image" src={url} alt={alt} title={path} />;
  return <span className="md-image-missing" title={failed ? `Could not load ${path}` : path}>[{alt || "image"}]</span>;
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
      case "image":
        return <Image key={i} alt={node.value.alt} path={node.value.path} />;
      case "later":
        return null;
    }
  });
}

function HunkCode({ label, text, diff }: { label: string; text: string; diff: Extract<NonNullable<Extract<Block, { type: "code" }>["value"]["diff"]>, { lines: unknown }> }) {
  const split = useSplit();
  const wrap = useWrap();
  return (
    <div className="md-code" data-raw={text}>
      <Head label={label} what="code" diff count={sourceLines(text).length} />
      <PatchBody patch={text} info={diff} split={split} wrap={wrap} fileLabels />
    </div>
  );
}

function Code({ value: { label, text, diff } }: { value: Extract<Block, { type: "code" }>["value"] }) {
  const count = sourceLines(text).length;
  const windowClass = count > SCROLL_WINDOW_LINES ? "scroll-window" : undefined;
  if (label === null && !windowClass) return <><pre><code>{text}</code></pre>{"\n"}</>;
  if (label !== null && diff && "lines" in diff) return <HunkCode label={label} text={text} diff={diff} />;
  const content = diff && "prefix" in diff
    ? <code>{diff.prefix.map((row, i) => <ColoredRow key={i} kind={row.kind}>{row.text}</ColoredRow>)}</code>
    : label === null || label === "code" ? <code>{text}</code> : <HighlightedCode text={text} lang={label} />;
  return (
    <div className="md-code">
      <Head label={label ?? "Code"} what="code" count={count} />
      <pre className={windowClass} style={windowClass ? scrollWindowStyle : undefined} data-patch={windowClass && (diff || label === "patch" || label === "diff") ? "" : undefined}>{content}</pre>{diff || label === "patch" ? null : "\n"}
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

/** Cut the rendered tree at its hard breaks and block boundaries, not at viewport wraps.
 *  A code or table window is one block; its own scrolling and copy still work. */
export function cutMarkdown(display: Display, limit: number): { display: Display; hidden: number } {
  let lines = 0;
  let hiddenBlockLines = 0;

  const takeInlines = (nodes: Inline[]): Inline[] => nodes.flatMap((node): Inline[] => {
    const visible = lines <= limit;
    if (node.type === "break") return ++lines <= limit ? [node] : [];
    const next = container(node, true);
    return visible && next ? [next] : [];
  });

  const takeBlocks = (nodes: Block[]): Block[] => nodes.flatMap((node): Block[] => {
    if (node.type === "blockquote" || node.type === "list") {
      const next = container(node);
      return next ? [next] : [];
    }

    const visible = ++lines <= limit;
    if (node.type === "paragraph" || node.type === "heading") {
      const next = container(node, true);
      return visible && next ? [next] : [];
    }
    if (!visible) {
      if (node.type === "code") hiddenBlockLines += Math.max(0, sourceLines(node.value.text).length - 1);
      if (node.type === "table") hiddenBlockLines += node.value.rows.length;
    }
    return visible ? [node] : [];
  });

  const container = <T extends Block | Inline>(node: T, keepEmpty = false) => mapCutContainer(
    node, takeBlocks, takeInlines, (items) => items.map(takeBlocks).filter((item) => item.length > 0), keepEmpty,
  );
  const preview = { blocks: takeBlocks(display.blocks) };
  return { display: preview, hidden: Math.max(0, lines - limit) + hiddenBlockLines };
}

export function MarkdownView({ display }: { display: Display }) {
  return <>{blocks(display.blocks)}</>;
}
