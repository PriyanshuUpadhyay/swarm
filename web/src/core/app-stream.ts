import { sleep, type RowFrame, type Snapshot, type SubscribeKey } from "./api";
import { queueRows, type Usage } from "./composer";
import { readSse, type SseEvent } from "./sse";
import { mergeRows } from "./transcript";
import { keepsEnded } from "./app-reader";
import { MAX_KEYS, type Chat, paneId } from "./app-types";
import type { App } from "./app";

/** How often live text types out, and in how many steps it catches up with the screen. */
const TYPE_MS = 30;
const TYPE_STEPS = 8;

export class AppStream {
  private typing: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly app: App) {}

  applySnapshot(snap: Snapshot) {
    this.app.snapshot = snap;
    // A card stays busy until its question goes; the same question shown again is a new ask.
    for (const [id, input] of this.app.inputs)
      if (input.answering && this.app.composing.chair(id).prompt?.id !== input.answering.prompt) input.answering = null;
    for (const chat of this.app.chats.values()) {
      const queue = queueRows(this.app.composing.chair(chat.id).queue);
      if (JSON.stringify(queue) === JSON.stringify(chat.queue)) continue;
      chat.queue = queue;
      chat.version++;
    }
    // A chair's and a child's log can show up after the chat opens.
    if (this.app.reading.open) this.app.panes.loadChair(this.app.reading.open);
    for (const id of this.app.visible) this.app.panes.loadPane(id);
    if (this.app.liveAll) void this.loadAllForLive();
  }

  // ---- stream ----

  async connect(signal: AbortSignal) {
    while (!signal.aborted) {
      try {
        await readSse(this.app.api.fetch, "/api/stream", this.app.api.headers(), (e) => this.onSse(e), signal);
      } catch {
        // Reconnect below.
      }
      this.app.streamId = null;
      this.app.sentKeys = "";
      if (!signal.aborted) {
        this.app.reconnecting = true;
        this.app.notify();
        await sleep(1000);
      }
    }
  }

  onSse(e: SseEvent) {
    let data: Record<string, unknown>;
    try {
      data = JSON.parse(e.data);
    } catch {
      return;
    }
    switch (e.event) {
      case "hello":
        this.app.reconnecting = false;
        // A frame sent while the stream was down is lost, so live text could be stale.
        for (const chat of this.app.chats.values()) {
          if (chat.live === null) continue;
          chat.live = null;
          chat.version++;
          chat.shift = false;
        }
        this.app.notify();
        if (this.app.serverBuild !== null && data.build !== this.app.serverBuild) return location.reload();
        this.app.serverBuild = String(data.build);
        this.app.streamId = String(data.stream);
        this.syncSubscriptions();
        void this.app.navigation.refreshSnapshot();
        break;
      case "snapshot":
        if (data.version !== this.app.snapshot?.version) void this.app.navigation.refreshSnapshot();
        break;
      case "rows": {
        const frame = data as unknown as RowFrame;
        const slot = this.app.pending.get(frame.key);
        if (slot) slot.push(frame);
        else this.app.pending.set(frame.key, [frame]);
        this.app.frame ||= requestAnimationFrame(() => this.flush());
        break;
      }
      case "live": {
        const chat = this.byKey(String(data.key));
        if (!chat) break;
        const text = typeof data.text === "string" ? data.text : null;
        // A turn can end seconds before its log's rows land, so its text stays until they do.
        if (text === null && keepsEnded(chat.rows, chat.live)) {
          chat.liveEnded = true;
          chat.liveShown = chat.live?.length ?? 0;
        } else {
          const shown = chat.live?.slice(0, chat.liveShown) ?? "";
          let same = 0;
          while (same < shown.length && text !== null && shown[same] === text[same]) same++;
          [chat.live, chat.liveShown, chat.liveEnded] = [text, same, false];
          if (text !== null && same < text.length) this.typeLive();
        }
        this.app.reading.changed(chat);
        break;
      }
      case "reset":
        void this.reset(String(data.key));
        break;
      case "reload":
        location.reload();
        break;
      case "show":
        // Fire-and-forget is safe: show awaits only refreshSnapshot, which keeps its own errors.
        if (typeof data.session === "string") void this.show(data.session);
        break;
      case "setup":
      case "handoff":
        this.app.life.onFrame(e.event, data);
        break;
    }
  }

  /** Shows more of each chat's live text every `TYPE_MS` until all of it shows. */
  private typeLive() {
    this.typing ??= setInterval(() => {
      let left = false;
      for (const chat of this.app.chats.values()) {
        const length = chat.live?.length ?? 0;
        if (chat.liveShown >= length) continue;
        chat.liveShown += Math.max(1, Math.ceil((length - chat.liveShown) / TYPE_STEPS));
        left ||= chat.liveShown < length;
        this.app.reading.changed(chat);
      }
      if (!left && this.typing !== null) {
        clearInterval(this.typing);
        this.typing = null;
      }
    }, TYPE_MS);
  }

  /** ADR 0102: a notice click opens its chat. A chat newer than the snapshot is read first. */
  async show(session: string) {
    if (!this.app.chatIds().includes(session)) await this.app.navigation.refreshSnapshot();
    if (this.app.chatIds().includes(session)) this.app.openChat(session);
  }

  /** Applies all buffered row frames once per animation frame. Only the open chat renders. */
  flush() {
    this.app.frame = 0;
    let render = false;
    for (const [key, frames] of this.app.pending) {
      const chat = this.byKey(key);
      if (!chat || chat.state !== "ready") continue;
      const before = chat.rows;
      for (const f of frames) {
        chat.rows = mergeRows(chat.rows, f.upsert, f.remove);
        chat.end = Math.max(chat.end, Number(f.end_offset));
        // Absent usage leaves the shown value; null clears it.
        if (f.usage !== undefined) {
          chat.usage = f.usage as Usage;
          this.app.inputRev++;
          if (chat.sessionId === this.app.openId) render = true;
        }
      }
      if (chat.rows !== before) {
        if (chat.liveEnded) [chat.live, chat.liveEnded] = [null, false];
        chat.version++;
        chat.shift = false;
        if (chat.sessionId === this.app.openId) render = true;
      }
    }
    this.app.pending.clear();
    if (render) this.app.notify();
  }

  /** The chat of a stream key. A child's key is its chat id; the chair's ends in `:chair`. */
  byKey(key: string): Chat | undefined {
    return this.app.chats.get(key) ?? (key.endsWith(":chair") ? this.app.chats.get(key.slice(0, -6)) : undefined);
  }

  async reset(key: string) {
    this.app.pending.delete(key);
    const chat = this.byKey(key);
    if (!chat) return;
    Object.assign(chat, this.app.reading.freshReaderState());
    await this.app.reading.loadTail(chat);
    this.app.sentKeys = "";
    this.syncSubscriptions();
    if (chat.sessionId === this.app.openId) this.app.notify();
  }

  async loadAllForLive() {
    const fresh = (this.app.snapshot?.sessions ?? []).filter((s) => !this.app.chats.has(s.id)).map((s) => this.app.reading.newChat(s.id));
    if (!fresh.length) return;
    await Promise.all(fresh.map((c) => this.app.reading.loadTail(c)));
    this.syncSubscriptions();
    if (fresh.some((c) => c.sessionId === this.app.openId)) this.app.notify();
  }

  /** Sends the full key set when it changes. Live mode holds every session; otherwise the LRU. */
  syncSubscriptions() {
    if (!this.app.streamId) return;
    const chairs = this.app.liveAll ? [...this.app.chats.values()].filter((c) => c.agent === "chair").map((c) => c.id) : this.app.lru;
    // A zoomed pane covers the strip, so only it follows its log.
    const panes = this.app.pane.zoomed && this.app.openId ? [paneId(this.app.openId, this.app.pane.zoomed)] : this.app.visible;
    const keys: SubscribeKey[] = [];
    for (const id of [...chairs, ...panes]) {
      const chat = this.app.chats.get(id);
      if (chat?.state === "ready") keys.push({ key: chat.key, from: chat.end, view: "rows" });
      if (keys.length === MAX_KEYS) break;
    }
    const sig = keys.map((k) => k.key).sort().join(",");
    if (sig === this.app.sentKeys) return;
    this.app.sentKeys = sig;
    this.app.api.subscribe(this.app.streamId, keys).catch(() => (this.app.sentKeys = ""));
  }
}
