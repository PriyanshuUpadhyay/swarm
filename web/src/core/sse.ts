// Server-sent events over fetch, because EventSource cannot send an Authorization header.

export type SseEvent = { event: string; data: string };

/** Incremental parser for the text/event-stream format. Feed it decoded text in any split. */
export class SseParser {
  private buf = "";
  private event = "";
  private data: string[] = [];

  push(chunk: string): SseEvent[] {
    this.buf += chunk;
    const out: SseEvent[] = [];
    let start = 0;
    for (;;) {
      const nl = this.buf.indexOf("\n", start);
      if (nl < 0) break;
      let line = this.buf.slice(start, nl);
      start = nl + 1;
      if (line.endsWith("\r")) line = line.slice(0, -1);
      if (line === "") {
        if (this.data.length) out.push({ event: this.event || "message", data: this.data.join("\n") });
        this.event = "";
        this.data = [];
        continue;
      }
      if (line.startsWith(":")) continue;
      const colon = line.indexOf(":");
      const field = colon < 0 ? line : line.slice(0, colon);
      let value = colon < 0 ? "" : line.slice(colon + 1);
      if (value.startsWith(" ")) value = value.slice(1);
      if (field === "event") this.event = value;
      else if (field === "data") this.data.push(value);
    }
    this.buf = this.buf.slice(start);
    return out;
  }
}

/** Reads one stream until it ends or fails. Resolves on a clean end; rejects on an HTTP error. */
export async function readSse(
  fetchFn: typeof fetch,
  url: string,
  headers: Record<string, string>,
  onEvent: (e: SseEvent) => void,
  signal: AbortSignal,
): Promise<void> {
  const res = await fetchFn(url, { headers: { ...headers, Accept: "text/event-stream" }, signal });
  if (!res.ok || !res.body) throw new Error(`${url}: ${res.status}`);
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  const parser = new SseParser();
  for (;;) {
    const { value, done } = await reader.read();
    if (done) return;
    for (const e of parser.push(value)) onEvent(e);
  }
}
