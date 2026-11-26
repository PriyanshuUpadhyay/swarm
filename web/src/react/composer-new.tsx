// The new chat composer (ADR 0149): one part, as the start page when no chat is open and as an
// overlay over the open chat. The owner types the first prompt, keeps or changes the project, the
// place and the run, and presses ⌘Enter. A starting chat shows its steps in `StartingView`.
// ↑ ↓ in an empty prompt, and ⌘↑ ⌘↓ always, change the project, and the prompt has the chat
// box's `/`, `$` and `@` menu (ADR 0155).

import { LoaderCircleIcon } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent } from "react";
import type { App } from "../core/app";
import { cliName, levelWord, profileRun, sameRun, type Run } from "../core/composer";
import { type Profile } from "../core/lifecycle";
import { DEFAULT_ROLE, defaultPlace, defaultRun, type ComposerRun, placeLabel, placeReady, defaultTarget, targets, type Place, type Target } from "../core/new-chat";
import { Button } from "../components/ui/button";
import { Kbd } from "../components/ui/kbd";
import { hasPendingImage, imagePaths } from "../core/images";
import { pick, toSend, type MenuRow } from "../core/slash";
import { dragOverFiles, dropImage, pasteImage, type Draft } from "./composer-image";
import { Chip, RunMenu, moveOption } from "./composer-new-parts";
import { CliMark } from "./cli-mark";
import { PreviewCard, usePreview } from "./composer-new-preview";
import { ImageThumbs } from "./images";
import { message } from "./lifecycle-shared";
import { PlacePanel } from "./place-lists";
import { PRESSED_PILL, runModel } from "./run-picker";
import { SlashMenu } from "./slash-menu";
import { useSlashMenu } from "./slash-menu-state";

/** A read of NewChat that also renders again when navSeq moves, because shown() and composing() read it. */
export function useNewChat<T>(app: App, read: () => T): T {
  useSyncExternalStore(app.subscribe, () => app.navSeq);
  return useSyncExternalStore(app.newChat.subscribe, read);
}

/** The start page: the composer on the open workspace's project, or on the one ⌘N picked. */
export function ComposerPage({ app }: { app: App }) {
  const composer = useNewChat(app, () => app.newChat.composing());
  const unsent = useNewChat(app, () => app.newChat.state.unsent);
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const workspace = useSyncExternalStore(app.subscribe, () => app.view.workspace);
  const list = targets(tree, composer?.cwd ?? workspace);
  const target = (composer && list.find((t) => t.cwd === composer.cwd)) || defaultTarget(list, tree, workspace);
  if (!target) return null;
  return <NewChatComposer app={app} key={composer?.seq ?? `page:${target.cwd}`} initial={target} place={composer?.place ?? defaultPlace(target)} text={composer?.text ?? unsent} run={composer?.run ?? null} focus={composer !== null} />;
}

/** The overlay over an open chat, while ⌘N or a "+" has it open. */
export function ComposerOverlay({ app }: { app: App }) {
  const composer = useNewChat(app, () => app.newChat.composing());
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  if (!composer) return null;
  const target = targets(tree, composer.cwd).find((t) => t.cwd === composer.cwd) ?? null;
  if (!target) return null;
  return <Overlay app={app} key={composer.seq} target={target} place={composer.place} text={composer.text} run={composer.run} />;
}

/** A modal dialog: Tab stays inside, focus goes back where it was on close, and a click on the
 *  backdrop closes it only while the prompt is empty, so a stray click keeps typed text. */
function Overlay({ app, target, place, text, run }: { app: App; target: Target; place: Place; text: string; run: ComposerRun | null }) {
  const root = useRef<HTMLDivElement>(null);
  // Read in the first render, before the composer's own effect moves the focus into it.
  const [before] = useState(() => (document.activeElement instanceof HTMLElement ? document.activeElement : null));
  useEffect(() => () => before?.focus(), [before]);

  const trap = (e: KeyboardEvent) => {
    if (e.key !== "Tab" || !root.current) return;
    const items = [...root.current.querySelectorAll<HTMLElement>("button:not([disabled]), textarea, input, [tabindex]:not([tabindex='-1'])")];
    const [first, last] = [items[0], items[items.length - 1]];
    if (e.shiftKey && document.activeElement === first) last?.focus();
    else if (!e.shiftKey && document.activeElement === last) first?.focus();
    else return;
    e.preventDefault();
  };

  const backdrop = (e: React.MouseEvent) => {
    const prompt = root.current?.querySelector<HTMLTextAreaElement>('[data-testid="new-chat-prompt"]');
    if (e.target !== e.currentTarget) return;
    // A press on the backdrop keeps the focus in the dialog, so its keys still work.
    e.preventDefault();
    if (!prompt?.value.trim()) app.newChat.close();
  };

  return (
    <div ref={root} role="dialog" data-state="open" aria-modal="true" aria-label="New chat" className="absolute inset-0 z-40 grid place-items-start justify-center bg-background/70 pt-[12vh] backdrop-blur-[2px]" data-testid="composer-overlay" onMouseDown={backdrop} onKeyDown={trap}>
      <NewChatComposer app={app} initial={target} place={place} text={text} run={run} focus overlay />
    </div>
  );
}

type Menu = "project" | "place" | "run" | null;

function NewChatComposer({ app, initial, place: firstPlace, text: firstText, run: firstRun, focus, overlay = false }: { app: App; initial: Target; place: Place; text: string; run: ComposerRun | null; focus: boolean; overlay?: boolean }) {
  const tree = useSyncExternalStore(app.subscribe, () => app.view.tree);
  const [cwd, setCwd] = useState(initial.cwd);
  const list = targets(tree, cwd);
  const target = list.find((t) => t.cwd === cwd) ?? initial;

  const [place, setPlace] = useState<Place>(firstPlace);
  const [text, setText] = useState(firstText);
  // The prompt now, for an image upload that ends after the owner typed more.
  const textNow = useRef(firstText);
  const prompt: Draft = {
    read: () => textNow.current,
    change: (next) => {
      textNow.current = next;
      setText(next);
      app.newChat.typed(next);
    },
  };
  const uploadTarget = useRef<Draft | null>(null);
  const imageDraft: Draft = {
    read: () => (uploadTarget.current ?? prompt).read(),
    change: (next) => (uploadTarget.current ?? prompt).change(next),
  };

  const [menu, setMenu] = useState<Menu>(firstPlace.kind === "branch" && !firstPlace.branch ? "place" : null);
  const [profiles, setProfiles] = useState<Profile[] | string | null>(null);
  // Edit and retry opens on the profile and run that the start posted.
  const [role, setRole] = useState(firstRun?.role ?? DEFAULT_ROLE);
  const [explicitProfile, setExplicitProfile] = useState(firstRun !== null && firstRun !== undefined);
  const [run, setRun] = useState<Run | null>(firstRun?.run ?? null);
  const [sending, setSending] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);
  const projects = useRef<HTMLDivElement>(null);
  const worktree = place.kind === "new" && target.git;
  const preview = usePreview(app, target, worktree);

  const profile = Array.isArray(profiles) ? (profiles.find((p) => p.name === role) ?? profiles[0]) : undefined;
  const base = profileRun(profile);
  const appDefault = explicitProfile ? null : defaultRun(app.view.tree?.prefs);
  const shown = run ?? appDefault ?? base;
  // The run on screen now, for a step that ends after an await.
  const shownNow = useRef(shown);
  useLayoutEffect(() => {
    shownNow.current = shown;
  });

  // The run chip's CLI lists the `/` rows; before the profiles arrive, no CLI is known.
  const provider = shown?.provider ?? null;
  const slash = useSlashMenu({
    text,
    provider,
    disabled: false,
    source: `${target.cwd}\n${provider}`,
    commands: async () => (provider ? app.newChat.commands(target.cwd, provider) : { commands: [], note: null }),
    files: () => app.newChat.files(target.cwd),
  });
  // After a pick the caret goes after the inserted text.
  const pickedCaret = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (pickedCaret.current === null) return;
    box.current?.setSelectionRange(pickedCaret.current, pickedCaret.current);
    pickedCaret.current = null;
  }, [text]);
  const insert = (row: MenuRow) => {
    if (!slash.menu) return;
    const next = pick(text, slash.menu, row);
    pickedCaret.current = next.caret;
    prompt.change(next.text);
    slash.setCaret(next.caret);
  };

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

  const lastMenu = useRef<Menu>(null);
  useEffect(() => {
    if (menu === "project") projects.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
    // The closed project menu took the focus with it; the prompt gets it, so the keys still work.
    else if (lastMenu.current === "project" && document.activeElement === document.body) box.current?.focus();
    lastMenu.current = menu;
  }, [menu]);

  const pickTarget = (next: Target) => {
    setCwd(next.cwd);
    setPlace(defaultPlace(next));
    setMenu(null);
  };

  const ready = placeReady(place) && !sending && !hasPendingImage(text);
  const send = async () => {
    // A second ⌘Enter while the first one posts does nothing.
    if (sending || hasPendingImage(text)) return;
    if (!ready) return setMenu("place");
    setSending(true);
    const changed = run && !sameRun(run, base) ? run : !run ? appDefault : null;
    // pi runs a skill only at the message's start, so the first prompt goes as the chat box sends it.
    const first = provider ? (await toSend(text, provider, slash.list, () => app.newChat.commands(target.cwd, provider))).text : text;
    const ok = await app.newChat.start(target.cwd, first, place, { role: profile?.name ?? role, run: changed }, {
      composer: true,
      submittedText: text,
      bindUpload: (draft) => { uploadTarget.current = draft; },
    });
    setSending(false);
    if (ok && textNow.current === text) {
      textNow.current = "";
      setText("");
    }
  };

  // ⌘⇧/ steps to the next thinking level the picked model takes. Each list read runs a CLI, so
  // only this key and the run menu read them.
  const cycleThinking = async () => {
    if (!shown) return;
    try {
      const [{ providers }, { models }] = await Promise.all([app.life.runs.providers(), app.life.runs.models(shown.provider)]);
      const { levels } = runModel({ providers, models: { [shown.provider]: models }, error: null }, shown);
      // The owner picked another run while the lists were read: that pick stays.
      if (!levels.length || !sameRun(shownNow.current, shown)) return;
      const at = shown.effort ? levels.indexOf(shown.effort) : -1;
      setRun({ ...shown, effort: levels[(at + 1) % levels.length] });
    } catch (err) {
      app.notice(message(err), true);
    }
  };

  /** The next or the previous project of the ⌘P list, around its ends; the place goes back to
   *  that project's default, and an open menu stays open. */
  const stepProject = (by: 1 | -1) => {
    const next = list[(list.findIndex((t) => t.cwd === target.cwd) + by + list.length) % list.length];
    if (!next || next.cwd === target.cwd) return;
    setCwd(next.cwd);
    setPlace(defaultPlace(next));
  };

  // The prompt's own keys, before the composer's: the open `/` menu, then the project arrows.
  const onPromptKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    const arrow = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    const command = e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    // ⌘ with an arrow changes the project also while the menu is open; the menu gets the plain arrows.
    let done = !(arrow && command) && slash.onKeyDown(e, insert);
    if (done) {
      // The menu took the key.
    } else if (slash.row && e.key === "Enter" && plain) {
      insert(slash.row);
      done = true;
    } else if (arrow && (command || (plain && text === ""))) {
      // With text in the prompt a plain arrow moves the caret, so only ⌘ changes the project there.
      stepProject(arrow);
      done = true;
    }
    if (done) {
      e.preventDefault();
      e.stopPropagation();
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const meta = e.metaKey || e.ctrlKey;
    let done = true;
    if (meta && e.key === "Enter") void send();
    else if (meta && (e.key === "/" || e.key === "?") && e.shiftKey) void cycleThinking();
    else if (meta && e.key === "/") setMenu((m) => (m === "run" ? null : "run"));
    else if (meta && !e.shiftKey && e.key.toLowerCase() === "p") setMenu((m) => (m === "project" ? null : "project"));
    else if (e.key === "Escape" && menu) setMenu(null);
    else if (e.key === "Escape") app.newChat.close(text);
    else done = false;
    if (done) {
      e.preventDefault();
      e.stopPropagation();
    }
  };

  const runText = shown ? [cliName(shown.provider), shown.model ?? "Default model", shown.effort && levelWord(shown.effort)].filter(Boolean).join(" · ") : typeof profiles === "string" ? `Profiles unavailable: ${profiles}` : "Reading profiles…";

  return (
    <div className={`grid w-full gap-3 ${overlay ? "max-w-[min(960px,calc(100vw-32px))]" : "max-w-[960px] px-4"}`} data-testid="new-chat-composer" data-cwd={target.cwd} data-overlay={overlay || undefined} onKeyDown={onKeyDown}>
      <div className="grid gap-3 min-[1000px]:grid-cols-[1fr_280px]">
        <div className="grid gap-2 rounded-2xl border border-border bg-surface p-3 has-[textarea:focus-visible]:ring-2 has-[textarea:focus-visible]:ring-ring">
          <ImageThumbs paths={imagePaths(text)} load={app.imageUrl} remove={(path) => prompt.change(text.split(path).join("").replace(/  +/g, " "))} />
          <div className="relative grid">
            <textarea
              ref={box}
              rows={overlay ? 4 : 5}
              data-testid="new-chat-prompt"
              aria-label="First prompt"
              placeholder={`What should this chat do in ${target.title}?`}
              className="min-h-[96px] w-full resize-none bg-transparent text-md outline-none"
              aria-expanded={slash.open}
              aria-controls={slash.open ? slash.listId : undefined}
              aria-activedescendant={slash.open && slash.rows.length ? `${slash.listId}-${slash.current}` : undefined}
              value={text}
              onChange={(e) => {
                prompt.change(e.target.value);
                slash.setCaret(e.target.selectionStart);
              }}
              onSelect={(e) => slash.setCaret(e.currentTarget.selectionStart)}
              onFocus={() => slash.setFocused(true)}
              onBlur={() => slash.setFocused(false)}
              onKeyDown={onPromptKey}
              onPaste={(e) => pasteImage(e, app, imageDraft)}
              onDragOver={dragOverFiles}
              onDrop={(e) => dropImage(e, app, imageDraft)}
            />
            {/* Under the box, because the overlay sits near the window's top. */}
            {slash.open && slash.menu && <SlashMenu id={slash.listId} menu={slash.menu} rows={slash.rows} current={slash.current} list={slash.shownList} below select={slash.setSelected} choose={insert} />}
          </div>
          <div className="flex flex-wrap items-center gap-1.5 text-sm">
            <Chip testid="chip-project" on={menu === "project"} title="Project (⌘↑ ⌘↓ change it, ⌘P lists)" onClick={() => setMenu((m) => (m === "project" ? null : "project"))}>
              {target.title}
            </Chip>
            <Chip testid="chip-place" on={menu === "place"} title={target.git ? "Where the chat runs" : "A plain folder runs the chat in itself"} disabled={!target.git} onClick={() => setMenu((m) => (m === "place" ? null : "place"))}>
              {placeLabel(place, target)}
            </Chip>
            <Chip testid="chip-run" on={menu === "run"} title="Run (⌘/), thinking (⌘⇧/)" onClick={() => setMenu((m) => (m === "run" ? null : "run"))}>
              {shown && <span className="mr-1.5"><CliMark provider={shown.provider} selected={menu === "run"} /></span>}
              {profile ? `${profile.name} · ` : ""}
              {runText}
            </Chip>
            <span className="flex-1" />
            <span className="text-xs text-muted-foreground">
              <Kbd>⌘↵</Kbd> start · <Kbd>↵</Kbd> new line · <Kbd>⌘↑↓</Kbd> project{overlay ? <> · <Kbd>Esc</Kbd> close</> : null}
            </span>
            <Button type="button" size="sm" data-testid="new-chat-start" disabled={!ready} onClick={() => void send()}>
              {sending && <LoaderCircleIcon className="animate-spin motion-reduce:animate-none" />}
              Start chat
            </Button>
          </div>
          {menu === "project" && (
            <div ref={projects} role="listbox" aria-label="Project" data-testid="project-menu" className="grid max-h-[240px] overflow-y-auto rounded-md border border-border" onKeyDown={moveOption}>
              {list.map((t) => (
                <button key={t.cwd} type="button" role="option" aria-selected={t.cwd === target.cwd} className="flex gap-2 px-2 py-1.5 text-left hover:bg-hover focus-visible:bg-hover aria-selected:bg-selected" onClick={() => pickTarget(t)}>
                  <span className="flex-1 truncate">{t.title}</span>
                  <span className="text-xs text-muted-foreground">{t.git ? "git" : "folder"}</span>
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
                      className={PRESSED_PILL}
                      onClick={() => {
                        setRole(p.name);
                        setExplicitProfile(true);
                        setRun(null);
                      }}
                    >
                      {p.name}
                    </button>
                  ))}
                </div>
              )}
              {shown && <RunMenu app={app} draft={shown} setDraft={setRun} />}
            </div>
          )}
        </div>
        <PreviewCard target={target} place={place} worktree={worktree} preview={preview} />
      </div>
    </div>
  );
}
