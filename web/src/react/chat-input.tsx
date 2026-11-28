import { TriangleAlertIcon } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { RunChip, useRunState } from "./run-chip";
import type { App, Banner, Composer } from "../core/app";
import { savedCarry, type AgentInput } from "../core/composer";
import { Button } from "../components/ui/button";
import { BackgroundLine } from "./background-line";
import { ComposerBox } from "./composer";
import { PromptCard } from "./prompt-card";
import { StatusLine } from "./status-line";

/** Below the transcript: the error banner, the agent's question, and the composer. */
export function ChatInput({ app, composer }: { app: App; composer: Composer }) {
  const { id, agent, state } = composer;
  const root = useRef<HTMLDivElement>(null);
  const promptId = agent.prompt?.id;
  const [arrival, setArrival] = useState<{ chat: string; prompt: string; activeDraft: boolean } | null>(null);

  useLayoutEffect(() => {
    if (promptId === undefined) {
      setArrival(null);
      return;
    }
    // Sample before disabling the textarea can move focus. This decision belongs to this prompt.
    const focused = document.activeElement;
    const own = focused?.matches('[data-testid="composer"]') && root.current?.contains(focused);
    const hasDraft = !!app.draft(id).trim();
    setArrival({ chat: id, prompt: promptId, activeDraft: !!own && hasDraft });
    if (!hasDraft && (own || (!agent.who && (!focused || focused === document.body)))) {
      const card = root.current?.querySelector<HTMLElement>('[data-testid="prompt-card"]');
      (card?.querySelector<HTMLInputElement>('[data-testid="prompt-text"]') ?? card)?.focus();
    }
  }, [app, id, promptId, agent.who]);

  const lockDraft = agent.waiting && arrival !== null && arrival.chat === id && arrival.prompt === promptId && !arrival.activeDraft;
  return (
    <div ref={root} className="chat-input shrink-0 px-6 pt-1 pb-4">
      <div className="mx-auto grid max-w-[760px] gap-2">
        <StatusLine key={`status:${id}`} app={app} composer={composer} />
        {!agent.who && <BackgroundLine key={`background:${id}`} session={id} />}
        {state.banner && <ChatBanner app={app} sessionId={id} banner={state.banner} />}
        {agent.prompt && <PromptCard app={app} sessionId={id} who={agent.who} prompt={agent.prompt} state={state} />}
        {!agent.unreachable && <RunChip.Pending app={app} sessionId={id} chair={agent} />}
        {/* An ended chair can pick its next run without starting it. */}
        {agent.who && agent.unreachable ? (
          <p className="text-xs text-muted-foreground" data-testid="composer-status">
            {agent.disabled}
          </p>
        ) : agent.unreachable ? (
          <EndedControls app={app} sessionId={id} chair={agent} />
        ) : (
          <ComposerBox key={id} app={app} composer={composer} lockDraft={lockDraft} />
        )}
      </div>
    </div>
  );
}

function EndedControls({ app, sessionId, chair }: { app: App; sessionId: string; chair: AgentInput }) {
  const state = useRunState(app);
  const run = state.endedRun?.[sessionId];
  const busy = state.resuming?.[sessionId] === true;
  const canResume = chair.resumable || chair.provider !== null;
  const saved = savedCarry(app.view.tree?.prefs);
  const carry = app.life.endedRunCarry(sessionId, run?.provider ?? chair.provider);
  const label = saved === "summary" && carry === "file" ? "Resume with File" : "Resume";

  return (
    <div className="grid justify-items-center gap-2">
      <div className="flex items-center gap-2">
        <RunChip app={app} sessionId={sessionId} chair={chair} ended />
        {canResume && <Button size="sm" data-testid="resume-chat" disabled={busy} onClick={() => void app.life.resumeChat(sessionId)}>
          {busy ? "Resuming…" : label}
        </Button>}
      </div>
      {canResume && <p className={`text-sm/4 text-muted-foreground ${state.switchModel === sessionId ? "invisible" : ""}`}>Resume goes on in this chat</p>}
    </div>
  );
}

type BannerView = (Banner & { tone?: "failed" }) | { text: string; tone: "neutral" };

function ChatBanner({ app, sessionId, banner }: { app: App; sessionId: string; banner: BannerView }) {
  const failed = banner.tone !== "neutral";
  return (
    <div role={failed ? "alert" : "status"} data-testid="chat-banner" className={`flex min-w-0 items-start gap-2 rounded-lg border p-1.5 ${failed ? "border-failed" : "border-menu-border"}`}>
      {failed && <TriangleAlertIcon className="mt-1 size-3.5 shrink-0 text-failed" aria-hidden />}
      <span className="min-w-0 flex-1 self-center font-mono text-sm leading-[18px] break-words whitespace-pre-wrap">{banner.text}</span>
      {failed && <>
        <Button variant="outline" size="sm" onClick={banner.retry}>Retry</Button>
        <Button variant="ghost" size="sm" onClick={() => app.dismissBanner(sessionId)}>Dismiss</Button>
      </>}
    </div>
  );
}

