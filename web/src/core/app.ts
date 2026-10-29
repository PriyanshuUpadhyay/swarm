// Framework-free app state. Each view layer reads `view`, which is a new object after every change.

import { Api, type Row, type RowFrame, type Snapshot } from "./api";
import { installKeys, Registry } from "./commands";
import { type AgentInput, type Prompt } from "./composer";
import { whenPainted } from "./perf";
import { focusOpenedChat, keepFocus } from "./focus";
import { chatFromPath, resolveChat } from "./route";
import { type Direction, type PaneCell } from "./panes";
import { type HistoryEntry } from "./history";
import { type Line } from "./transcript";
import { Lifecycle } from "./lifecycle";
import { type CommandList, type SkillPick } from "./slash";
import { Settings } from "./settings";
import { Skills } from "./skills";
import { Notices } from "./notices";
import { chatRows, type ChoiceOp, type NavRow, type PaletteItem, type RecentChat, type Tabs, type TreeName, type TreeView } from "./nav";
import { type Chat, type Input, type Find, type Strip, type View, type Reader, type Ask, NO_FIND, NO_CELLS, SIDEBAR_VIEW_KEY, readStore } from "./app-types";
import { AppNavigation } from "./app-navigation";
import { AppSidebar } from "./app-sidebar";
import { AppReader } from "./app-reader";
import { AppComposer } from "./app-composer";
import { AppPanes } from "./app-panes";
import { AppStream } from "./app-stream";
import { AppActions } from "./app-actions";

export { TAIL, LRU_SIZE, type FullText, type Banner, type Input, type Composer, type Find, type Pane, type Strip, type View, type Reader, type Ask, SIDEBAR_VIEWS, paneId, PROMPT_JUMP, SENT } from "./app-types";

export class App {
  readonly navigation = new AppNavigation(this);
  readonly sidebar = new AppSidebar(this);
  readonly reading = new AppReader(this);
  readonly composing = new AppComposer(this);
  readonly panes = new AppPanes(this);
  readonly streaming = new AppStream(this);
  readonly actions = new AppActions(this);

  readonly registry = new Registry();
  /** New workspace, switch model, archive, delete and Archived chats (S3). */
  readonly life = new Lifecycle(this);
  /** Usage meters and Settings (S5). */
  readonly settings = new Settings(this);
  /** The kit's blocks and skills (ADR 0111). */
  readonly skills = new Skills(this);
  /** Whether macOS lets swarm's notices through (ADR 0102). */
  readonly notices = new Notices(this);
  view: View;
  snapshot: Snapshot | null = null;
  tree: TreeView | null = null;
  archivedTree: TreeView | null = null;
  treeName: TreeName = "workspaces";
  rows: NavRow[] = [];
  /** Fold presses the server has not confirmed yet, by fold id. */
  folds = new Map<string, boolean>();
  selected: string | null = null;
  marked: ReadonlySet<string> = new Set();
  openId: string | null = null;
  workspace: string | null = null;
  tabs: Tabs | null = null;
  sidebarView = readStore(SIDEBAR_VIEW_KEY) === "2" ? 2 : 1;
  recentMemo: { tree: TreeView | null; snapshot: Snapshot | null; list: RecentChat[] } | null = null;
  recentSelected: string | null = null;
  /** The choices revision of the last read or write. Writes go one at a time, each on the last. */
  revision: string | null = null;
  writes: Promise<unknown> = Promise.resolve();
  writing = 0;
  /** Bumped by each write, so a read that started before it does not take back its revision. */
  writeSeq = 0;
  restored = false;
  /** The chat that the page's URL named when it opened (ADR 0088), and whether the URL has
   *  followed the open chat yet; the first follow replaces the URL and later ones push. */
  readonly urlChat = typeof location === "undefined" ? null : chatFromPath(location.pathname);
  urlFollowed = false;
  /** Bumped by each move between chats, so a slow workspace step does not undo a newer one. */
  navSeq = 0;
  starting = false;
  paletteOpen = false;
  branchPicking = false;
  asking: Ask | null = null;
  error: string | null = null;
  chats = new Map<string, Chat>();
  lru: string[] = [];
  listeners = new Set<() => void>();
  streamId: string | null = null;
  /** The server build of the first stream; a later stream from another build reloads the page. */
  serverBuild: string | null = null;
  sentKeys = "";
  pending = new Map<string, RowFrame[]>();
  find: Find = NO_FIND;
  /** The chat that Find searches: the focused pane's when Find opened, else the chair's. */
  findId: string | null = null;
  findVersion = "";
  frame = 0;
  snapshotBusy: Promise<void> | null = null;
  snapshotAgain = false;
  inputs = new Map<string, Input>();
  drafts = new Map<string, string>();
  picks = new Map<string, SkillPick[]>();
  histories = new Map<string, HistoryEntry[]>();
  stashes = new Map<string, string>();
  alreadySentSeq = 0;
  inputRev = 0;
  pane = { focused: null as string | null, zoomed: null as string | null, reveal: null as string | null, revealSeq: 0 };
  /** Chat ids of the child columns on screen. Only these follow their logs. */
  visible = new Set<string>();
  cellsMemo: { snapshot: Snapshot | null; openId: string | null; cells: PaneCell[] } | null = null;
  strip: Strip = { cells: NO_CELLS, panes: new Map(), focused: null, zoomed: null, reveal: null, revealSeq: 0 };
  readers = new Map<string, Reader>();
  /** Shows a short message to the owner; the view layer sets it (a toast). */
  notice: (text: string, error?: boolean, action?: { label: string; run: () => void }) => void = () => {};
  /** The server refused the token. */
  unauthorized: () => void = () => {};

  constructor(
    readonly api: Api,
    readonly liveAll = false,
  ) {
    this.view = this.panes.makeView();
    this.registry.add(...this.actions.commands(), ...this.settings.commands(), ...this.skills.commands());
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  notify() {
    this.navigation.followUrl();
    this.view = this.panes.makeView();
    for (const fn of this.listeners) fn();
  }

  start(): () => void {
    window.__swarmPerf = [];
    const uninstall = installKeys(this.registry);
    const unkeep = keepFocus();
    this.navigation.focusOpened = focusOpenedChat;
    const abort = new AbortController();
    // Back and forward open the chat that the URL names, or leave the open one.
    addEventListener(
      "popstate",
      () => {
        const id = chatFromPath(location.pathname);
        const chat = id && resolveChat(id, this.chatIds());
        if (chat) this.navigation.openChat(chat, undefined, { history: false });
        else if (this.openId) this.navigation.leaveChat(this.openId);
      },
      { signal: abort.signal },
    );
    if (location.pathname === "/setup") this.settings.show("setup");
    // Closing Setup & doctor gives the URL back to the open chat.
    abort.signal.addEventListener("abort", this.settings.subscribe(() => this.navigation.followUrl()));
    void this.navigation.refreshSnapshot().then(() => {
      whenPainted(
        // The sidebar view is saved, so the page can start on Recent, which has no tree.
        () => document.querySelector('[data-testid="sidebar"][data-loaded="true"], [data-testid="recent"]') !== null,
        () => (window.__swarmReady = true),
      );
    });
    void this.streaming.connect(abort.signal);
    return () => {
      uninstall();
      unkeep();
      abort.abort();
    };
  }

  // ---- snapshot, tree and tabs ----

  /** Reads the snapshot, tree and tabs again, after a write outside the choices. */
  refresh(): Promise<void> {
    return this.navigation.refreshSnapshot();
  }

  /** Whether the sidebar lists the chat. */
  listsChat(session: string) {
    return chatRows(this.tree).some((r) => r.selection?.chat === session);
  }

  /** Every session id that the snapshot lists. */
  chatIds(): string[] {
    return this.snapshot?.sessions.map((s) => s.id) ?? [];
  }

  /** A session's snapshot row and its agents. */
  agentsOf(session: string) {
    return { info: this.snapshot?.sessions.find((s) => s.id === session), agents: this.snapshot?.agents[session] ?? [] };
  }

  refreshSnapshot(): Promise<void> {
    return this.navigation.refreshSnapshot();
  }

  applySnapshot(snap: Snapshot) {
    return this.streaming.applySnapshot(snap);
  }

  choose(op: ChoiceOp): Promise<boolean> {
    return this.navigation.choose(op);
  }

  toggle(id: string) {
    return this.sidebar.toggle(id);
  }

  unarchiving = new Set<string>();

  unarchive(session: string) {
    return this.sidebar.unarchive(session);
  }

  clickRow(id: string, pick: "toggle" | "range" | null = null) {
    return this.sidebar.clickRow(id, pick);
  }

  clickRecent(session: string) {
    return this.sidebar.clickRecent(session);
  }

  dropRecent(session: string, target: string) {
    return this.sidebar.dropRecent(session, target);
  }

  pin(workspace: string) {
    return this.sidebar.pin(workspace);
  }

  workspaceMoveTarget(path: string, offset: -1 | 1): string | null {
    return this.sidebar.workspaceMoveTarget(path, offset);
  }

  setTreeName(name: TreeName) {
    return this.sidebar.setTreeName(name);
  }

  setSidebarView(n: number) {
    return this.sidebar.setSidebarView(n);
  }

  showHome() {
    return this.navigation.showHome();
  }

  selectWorkspace(path: string, e?: KeyboardEvent) {
    return this.navigation.selectWorkspace(path, e);
  }

  startChat(workspace = this.workspace, launch: { role?: string } = {}, draft = "") {
    return this.actions.startChat(workspace, launch, draft);
  }

  markedChats(): string[] {
    return this.sidebar.markedChats();
  }

  clearMarked() {
    return this.sidebar.clearMarked();
  }

  leaveChat(session: string) {
    return this.navigation.leaveChat(session);
  }

  leaveWorkspace(workspace: string) {
    return this.navigation.leaveWorkspace(workspace);
  }

  chatTitle(session: string): string {
    return this.navigation.chatTitle(session);
  }

  openWaiting(e?: KeyboardEvent) {
    return this.actions.openWaiting(e);
  }

  /** Sessions of closed tabs, the last closed at the end, for ⇧⌘T. */
  closedTabs: string[] = [];

  reopenTab() {
    return this.navigation.reopenTab();
  }

  closeTab(key?: string) {
    return this.navigation.closeTab(key);
  }

  openChat(sessionId: string, e?: KeyboardEvent, opts: { write?: boolean; history?: boolean } = {}) {
    return this.navigation.openChat(sessionId, e, opts);
  }

  reader(id: string): Reader {
    return this.reading.reader(id);
  }

  toggleLine(line: Line, id?: string) {
    return this.reading.toggleLine(line, id);
  }

  loadFull(previewId: string, id?: string) {
    return this.reading.loadFull(previewId, id);
  }

  images = new Map<string, Promise<string>>();

  imageUrl = (path: string): Promise<string> => this.reading.imageUrl(path);

  fullText(previewId: string, id?: string): Promise<string> {
    return this.reading.fullText(previewId, id);
  }

  loadOlder(id?: string) {
    return this.reading.loadOlder(id);
  }

  openFind() {
    return this.reading.openFind();
  }

  closeFind() {
    return this.reading.closeFind();
  }

  setFindQuery(query: string) {
    return this.reading.setFindQuery(query);
  }

  stepFind(dir: number) {
    return this.reading.stepFind(dir);
  }

  chair(id: string): AgentInput {
    return this.composing.chair(id);
  }

  draft(sessionId: string): string {
    return this.composing.draft(sessionId);
  }

  draftPicks(sessionId: string): SkillPick[] {
    return this.composing.draftPicks(sessionId);
  }

  setDraft(sessionId: string, text: string, picks?: readonly SkillPick[]) {
    return this.composing.setDraft(sessionId, text, picks);
  }

  history(sessionId: string): string[] {
    return this.composing.history(sessionId);
  }

  stashed(sessionId: string): string {
    return this.composing.stashed(sessionId);
  }

  setStash(sessionId: string, text: string) {
    return this.composing.setStash(sessionId, text);
  }

  reuse(sessionId: string, text: string) {
    return this.composing.reuse(sessionId, text);
  }

  send(sessionId: string, text: string, draftText = text, owner = true): Promise<boolean> {
    return this.composing.send(sessionId, text, draftText, owner);
  }

  menuCommands(sessionId: string): Promise<CommandList> {
    return this.composing.menuCommands(sessionId);
  }

  menuFiles(sessionId: string): Promise<CommandList> {
    return this.composing.menuFiles(sessionId);
  }

  interrupt(sessionId: string) {
    return this.composing.interrupt(sessionId);
  }

  answer(sessionId: string, prompt: Prompt, choice: number) {
    return this.composing.answer(sessionId, prompt, choice);
  }

  dismissBanner(sessionId: string) {
    return this.composing.dismissBanner(sessionId);
  }

  pullBack(sessionId: string) {
    return this.composing.pullBack(sessionId);
  }

  setPaneVisible(agent: string, visible: boolean) {
    return this.panes.setPaneVisible(agent, visible);
  }

  focusPane(agent: string) {
    return this.panes.focusPane(agent);
  }

  paneFocused(agent: string | null) {
    return this.panes.paneFocused(agent);
  }

  movePaneFocus(direction: Direction): boolean {
    return this.panes.movePaneFocus(direction);
  }

  toggleZoom(agent?: string) {
    return this.panes.toggleZoom(agent);
  }

  stopPane(agent: string): Promise<void> {
    return this.panes.stopPane(agent);
  }

  closePane(agent: string): Promise<void> {
    return this.panes.closePane(agent);
  }

  /** Branch from here on a user row of the open chat. */
  branch(row: string) {
    if (this.openId) void this.actions.branch(this.openId, row);
  }

  /** The Esc Esc picker of the open chat's messages to branch from. */
  setBranchPicker(open: boolean) {
    return this.actions.setBranchPicker(open);
  }

  /** The open chat's loaded rows; the array is new whenever they change. */
  openRows(): readonly Row[] | undefined {
    return this.reading.chat()?.rows;
  }

  /** Loads the open chat's rows again, for a picker whose load failed. */
  async reloadOpenRows() {
    const chat = this.reading.chat();
    if (!chat) return;
    chat.state = "loading";
    this.notify();
    await this.reading.loadTail(chat);
    this.streaming.syncSubscriptions();
    this.notify();
  }

  /** The user rows of `rows` (the open chat's by default), newest first. */
  branchRows(rows: readonly Row[] = this.openRows() ?? []): Row[] {
    return rows.filter((r) => r.kind === "user").reverse();
  }

  setPalette(open: boolean) {
    return this.actions.setPalette(open);
  }

  ask(ask: Ask | null) {
    return this.actions.ask(ask);
  }

  renameChat(session: string) {
    return this.actions.renameChat(session);
  }

  renameWorkspace(path: string) {
    return this.actions.renameWorkspace(path);
  }

  renameProject(path: string, title: string) {
    return this.actions.renameProject(path, title);
  }

  paletteItems(): PaletteItem[] {
    return this.actions.paletteItems();
  }

  flush() {
    return this.streaming.flush();
  }
}
