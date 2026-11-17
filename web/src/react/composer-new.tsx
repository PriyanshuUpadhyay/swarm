// The new chat composer (ADR 0147): one part, as the start page when no chat is open and as an
// overlay over the open chat. The owner types the first prompt, keeps or changes the project, the
// place and the run, and presses ⌘Enter. A starting chat shows its steps in `StartingView`.

import { CircleCheckIcon, CircleIcon, LoaderCircleIcon, TriangleAlertIcon } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent } from "react";
import type { App } from "../core/app";
import { cliName, levelWord, type Run } from "../core/composer";
import { setupFailure, type Profile } from "../core/lifecycle";
import { defaultPlace, defaultTarget, STEP_WORDS, targets, type Place, type Starting, type Target } from "../core/new-chat";
import { Button } from "../components/ui/button";
import { Kbd } from "../components/ui/kbd";
import { message } from "./lifecycle-shared";
import { PlacePanel, placeLabel, placeReady } from "./place-lists";
import { RunPicker, runModel, useRunCatalog } from "./run-picker";

type Preview = { place: "folder" } | { place: "worktree"; branch: string | null; folder: string | null; base: string | null; fetchedUnixS: number | null; setup: string | null };

/** The profile a new chat starts on unless the owner picks another: `chat`, else the first. */
const DEFAULT_ROLE = "chat";

/** A profile's run: its first runner, on the CLI's login. */
function profileRun(profile: Profile | undefined): Run | null {
  const r = profile?.runners[0] as (Profile["runners"][number] & { effort?: string }) | undefined;
  return r ? { provider: r.provider, model: r.model ?? null, effort: r.effort ?? null, account: null } : null;
}

const sameRun = (a: Run | null, b: Run | null) => a?.provider === b?.provider && a?.model === b?.model && a?.effort === b?.effort && a?.account === b?.account;

function useNewChat<T>(app: App, read: () => T): T {
  useSyncExternalStore(app.subscribe, () => app.navSeq);
  return useSyncExternalStore(app.newChat.subscribe, read);
}

/** The start page: the composer on the open workspace's project, or on the one ⌘N picked. */
export function ComposerPage({ app }: { app: App }) {
  const composer = useNewChat(app, () => app.newChat.state.composer);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const workspace = useSyncExternalStore(app.subscribe, () => app.view.workspace);
  const list = targets(tree, workspace);
  const target = (composer && list.find((t) => t.cwd === composer.cwd)) || defaultTarget(list, tree, workspace);
  if (!target) return null;
  return <NewChatComposer app={app} key={composer?.seq ?? `page:${target.cwd}`} initial={target} place={composer?.place ?? defaultPlace(target)} focus={composer !== null} />;
}

/** The overlay over an open chat, while ⌘N or a "+" has it open. */
export function ComposerOverlay({ app }: { app: App }) {
  const composer = useNewChat(app, () => app.newChat.state.composer);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  if (!composer) return null;
  const target = targets(tree, composer.cwd).find((t) => t.cwd === composer.cwd) ?? null;
  if (!target) return null;
  return (
    <div className="absolute inset-0 z-40 grid place-items-start justify-center bg-background/70 pt-[12vh] backdrop-blur-[2px]" data-testid="composer-overlay" onMouseDown={(e) => e.target === e.currentTarget && app.newChat.close()}>
      <NewChatComposer app={app} key={composer.seq} initial={target} place={composer.place} focus overlay />
    </div>
  );
}

type Menu = "project" | "place" | "run" | null;

function NewChatComposer({ app, initial, place: firstPlace, focus, overlay = false }: { app: App; initial: Target; place: Place; focus: boolean; overlay?: boolean }) {
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const [cwd, setCwd] = useState(initial.cwd);
  const list = targets(tree, cwd);
  const target = list.find((t) => t.cwd === cwd) ?? initial;
  const [place, setPlace] = useState<Place>(firstPlace);
  const [text, setText] = useState("");
  const [menu, setMenu] = useState<Menu>(firstPlace.kind === "branch" && !firstPlace.branch ? "place" : null);
  const [profiles, setProfiles] = useState<Profile[] | string | null>(null);
  const [role, setRole] = useState(DEFAULT_ROLE);
  const [run, setRun] = useState<Run | null>(null);
  const [sending, setSending] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);
  const preview = usePreview(app, target, place, text);
  const profile = Array.isArray(profiles) ? (profiles.find((p) => p.name === role) ?? profiles[0]) : undefined;
  const base = profileRun(profile);
  const shown = run ?? base;
  const catalog = useRunCatalog(app, shown?.provider ?? "claude");

  useEffect(() => {
    app.life.profiles().then(
      (list) => {
        setProfiles(list);
        setRole((r) => (list.some((p) => p.name === r) ? r : (list[0]?.name ?? r)));
      },
      (err) => setProfiles(message(err)),
    );
  }, [app]);
  useEffect(() => {
    if (focus) box.current?.focus();
  }, [focus]);

  const pickTarget = (next: Target) => {
    setCwd(next.cwd);
    setPlace(defaultPlace(next));
    setMenu(null);
  };
  const ready = placeReady(place) && !sending;
  const send = async () => {
    if (!ready) return setMenu("place");
    setSending(true);
    const changed = run && !sameRun(run, base) ? run : null;
    const ok = await app.newChat.start(target.cwd, text, place, { role: profile?.name ?? role, run: changed });
    if (!ok) setSending(false);
  };
  // ⌘⇧/ steps to the next thinking level the picked model takes.
  const cycleThinking = () => {
    if (!shown) return;
    const { levels } = runModel(catalog, shown);
    if (!levels.length) return;
    const at = shown.effort ? levels.indexOf(shown.effort) : -1;
    setRun({ ...shown, effort: levels[(at + 1) % levels.length] });
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const meta = e.metaKey || e.ctrlKey;
    let done = true;
    if (meta && e.key === "Enter") void send();
    else if (meta && (e.key === "/" || e.key === "?") && e.shiftKey) cycleThinking();
    else if (meta && e.key === "/") setMenu((m) => (m === "run" ? null : "run"));
    else if (meta && !e.shiftKey && e.key.toLowerCase() === "p") setMenu((m) => (m === "project" ? null : "project"));
    else if (e.key === "Escape" && menu) setMenu(null);
    else if (e.key === "Escape") app.newChat.close();
    else done = false;
    if (done) {
      e.preventDefault();
      e.stopPropagation();
    }
  };

  const runText = shown ? [cliName(shown.provider), shown.model ?? "Default model", shown.effort && levelWord(shown.effort)].filter(Boolean).join(" · ") : typeof profiles === "string" ? "Profiles unavailable" : "Reading profiles…";
  return (
    <div className={`grid w-full gap-3 ${overlay ? "max-w-[min(960px,calc(100vw-32px))]" : "max-w-[960px] px-4"}`} data-testid="new-chat-composer" data-overlay={overlay || undefined} onKeyDown={onKeyDown}>
      <div className="grid gap-3 min-[1000px]:grid-cols-[1fr_280px]">
        <div className="grid gap-2 rounded-xl border border-border-strong bg-popover p-3 shadow-panel">
          <textarea
            ref={box}
            rows={overlay ? 4 : 5}
            data-testid="new-chat-prompt"
            aria-label="First prompt"
            placeholder={`What should this chat do in ${target.title}?`}
            className="min-h-[96px] w-full resize-none bg-transparent outline-none"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
          <div className="flex flex-wrap items-center gap-1.5 text-sm">
            <Chip testid="chip-project" on={menu === "project"} title="Project (⌘P)" onClick={() => setMenu((m) => (m === "project" ? null : "project"))}>
              {target.title}
            </Chip>
            <Chip testid="chip-place" on={menu === "place"} title={target.git ? "Where the chat runs" : "A plain folder runs the chat in itself"} disabled={!target.git} onClick={() => setMenu((m) => (m === "place" ? null : "place"))}>
              {placeLabel(place, target)}
            </Chip>
            <Chip testid="chip-run" on={menu === "run"} title="Run (⌘/), thinking (⌘⇧/)" onClick={() => setMenu((m) => (m === "run" ? null : "run"))}>
              {profile ? `${profile.name} · ` : ""}
              {runText}
            </Chip>
            <span className="flex-1" />
            <span className="text-xs text-muted-foreground">
              <Kbd>⌘↵</Kbd> start · <Kbd>↵</Kbd> new line{overlay ? <> · <Kbd>Esc</Kbd> close</> : null}
            </span>
            <Button type="button" size="sm" data-testid="new-chat-start" disabled={!ready} onClick={() => void send()}>
              {sending && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
              Start chat
            </Button>
          </div>
          {menu === "project" && (
            <div role="listbox" aria-label="Project" data-testid="project-menu" className="grid max-h-[240px] overflow-y-auto rounded-md border border-border">
              {list.map((t) => (
                <button key={t.cwd} type="button" role="option" aria-selected={t.cwd === target.cwd} className="flex gap-2 px-2 py-1.5 text-left hover:bg-hover aria-selected:bg-selected" onClick={() => pickTarget(t)}>
                  <span className="flex-1 truncate">{t.title}</span>
                  <span className="text-xs text-muted-foreground">{t.worktree ? "new worktree" : t.git ? "git" : "folder"}</span>
                </button>
              ))}
            </div>
          )}
          {menu === "place" && target.git && <PlacePanel app={app} target={target} place={place} onPlace={setPlace} />}
          {menu === "run" && (
            <div className="grid gap-3" data-testid="new-chat-run">
              {Array.isArray(profiles) && profiles.length > 1 && (
                <div className="flex flex-wrap gap-1" role="group" aria-label="Profile">
                  {profiles.map((p) => (
                    <button
                      key={p.name}
                      type="button"
                      data-testid="profile-item"
                      aria-pressed={p.name === profile?.name}
                      className="rounded-md border border-border px-2 py-1 text-xs hover:bg-hover aria-pressed:border-foreground aria-pressed:bg-foreground aria-pressed:text-background"
                      onClick={() => {
                        setRole(p.name);
                        setRun(null);
                      }}
                    >
                      {p.name}
                    </button>
                  ))}
                </div>
              )}
              {shown && <RunPicker app={app} catalog={catalog} current={base ?? shown} draft={shown} setDraft={setRun} />}
            </div>
          )}
        </div>
        <PreviewCard target={target} place={place} preview={preview} />
      </div>
    </div>
  );
}

function Chip({ testid, on, title, disabled, onClick, children }: { testid: string; on: boolean; title: string; disabled?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" data-testid={testid} aria-expanded={on} title={title} disabled={disabled} className="max-w-[320px] truncate rounded-md border border-border px-2 py-1 hover:bg-hover aria-expanded:bg-selected disabled:pointer-events-none" onClick={onClick}>
      {children}
    </button>
  );
}

/** `/api/chat/new-preview`, read 250 ms after the typing stops, for a new worktree only. */
function usePreview(app: App, target: Target, place: Place, text: string): Preview | null {
  const [preview, setPreview] = useState<Preview | null>(null);
  const wanted = place.kind === "new" && target.worktree;
  useEffect(() => {
    if (!wanted) return setPreview(null);
    let live = true;
    const timer = setTimeout(() => {
      app.api.get<Preview>("/api/chat/new-preview", { cwd: target.cwd, prompt: text }).then(
        (p) => live && setPreview(p),
        () => live && setPreview(null), // The card is a hint; the send checks the name again.
      );
    }, 250);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [app, target.cwd, text, wanted]);
  return wanted ? preview : null;
}

function PreviewCard({ target, place, preview }: { target: Target; place: Place; preview: Preview | null }) {
  const rows: [string, string][] =
    preview?.place === "worktree"
      ? [
          ["Branch", preview.branch ?? "swarm/chat-…, renamed from the first prompt"],
          ["Folder", preview.folder ?? "named from the branch"],
          ["Base", preview.base ?? "none"],
          ["Fetched", preview.fetchedUnixS ? new Date(preview.fetchedUnixS * 1000).toLocaleString() : "never"],
          ...(preview.setup ? ([["Setup", preview.setup]] as [string, string][]) : []),
        ]
      : place.kind === "new" && target.worktree
        ? [["Branch", "Reading…"]]
        : [["Folder", place.kind === "here" || !target.git ? target.cwd : placeLabel(place, target)]];
  return (
    <aside className="grid content-start gap-1.5 rounded-xl border border-border p-3 text-sm" aria-label="What starts" data-testid="new-chat-preview">
      <h3 className="text-xs font-medium text-muted-foreground">{place.kind === "new" && target.worktree ? "A new worktree" : "Where the chat runs"}</h3>
      {rows.map(([k, v]) => (
        <div key={k} className="grid grid-cols-[64px_1fr] gap-2">
          <span className="text-muted-foreground">{k}</span>
          <span className="truncate font-mono text-xs leading-5" data-testid={`preview-${k.toLowerCase()}`} title={v}>
            {v}
          </span>
        </div>
      ))}
    </aside>
  );
}

/** A starting chat's view: its steps and setup output, and a box whose text goes after the first prompt. */
export function StartingView({ app, start }: { app: App; start: Starting }) {
  const [text, setText] = useState("");
  const steps = ["worktree", "setup", "start", "send"].map((step) => start.steps.find((s) => s.step === step)).filter((s) => s !== undefined);
  const queue = () => {
    app.newChat.queue(start.id, text);
    setText("");
  };
  return (
    <div className="flex flex-1 flex-col items-center gap-4 overflow-y-auto p-6" data-testid="starting-chat" data-state={start.error ? "failed" : "starting"}>
      <div className="grid w-full max-w-[720px] gap-3">
        <h2 className="truncate text-lg font-semibold">{start.title}</h2>
        {start.prompt && <p className="line-clamp-4 text-sm whitespace-pre-wrap text-muted-foreground">{start.prompt}</p>}
        <ol className="grid gap-1.5" aria-label="Start steps" data-testid="start-steps">
          {steps.length === 0 && !start.error && <Step state="running" text="Getting ready" />}
          {steps.map((s) => (
            <Step key={s.step} state={s.state} text={`${STEP_WORDS[s.step] ?? s.step}${s.step === "worktree" && start.branch ? `: ${start.branch}` : ""}`} error={s.error} />
          ))}
        </ol>
        {start.setup && (
          <pre data-testid="setup-output" aria-label="Setup output" className="max-h-[260px] overflow-auto rounded-md border border-border bg-code p-2 font-mono text-xs break-words whitespace-pre-wrap select-text">
            {start.setup.output || (start.setup.state === "running" ? "Waiting for output…" : "(no output)")}
          </pre>
        )}
        {start.setup && start.setup.state !== "running" && start.setup.state !== "completed" && <p className="text-sm text-failed">{setupFailure(start.setup)}</p>}
        {start.error ? (
          <div className="grid gap-2">
            <p className="text-sm break-words text-failed" role="alert" data-testid="start-error">
              {start.error}
            </p>
            <div>
              <Button type="button" size="sm" variant="outline" onClick={() => app.newChat.dismiss(start.id)}>
                Close
              </Button>
            </div>
          </div>
        ) : (
          <div className="grid gap-1.5">
            <textarea
              rows={3}
              aria-label="Next message"
              data-testid="starting-prompt"
              placeholder="Type the next message. It goes after the first prompt."
              className="w-full resize-none rounded-lg border border-border-strong bg-popover p-2 outline-none"
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                  e.preventDefault();
                  queue();
                }
              }}
            />
            {start.queued.length > 0 && (
              <p className="text-xs text-muted-foreground" data-testid="start-queued">
                {start.queued.length === 1 ? "1 message waits" : `${start.queued.length} messages wait`} for the chat to start.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function Step({ state, text, error }: { state: "running" | "completed" | "failed"; text: string; error?: string | null }) {
  return (
    <li className="flex items-start gap-2 text-sm" data-state={state}>
      {state === "completed" ? (
        <CircleCheckIcon className="mt-0.5 size-4 text-done" aria-label="Done" />
      ) : state === "failed" ? (
        <TriangleAlertIcon className="mt-0.5 size-4 text-failed" aria-label="Failed" />
      ) : state === "running" ? (
        <LoaderCircleIcon className="mt-0.5 size-4 animate-spin motion-reduce:animate-none" aria-label="Running" />
      ) : (
        <CircleIcon className="mt-0.5 size-4 text-muted-foreground" />
      )}
      <span className="min-w-0 break-words">
        {text}
        {error && <span className="block text-xs text-failed">{error}</span>}
      </span>
    </li>
  );
}
