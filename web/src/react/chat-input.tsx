import { TriangleAlertIcon } from "lucide-react";
import { useSyncExternalStore } from "react";
import { RunChip } from "./run-chip";
import type { App, Banner, Composer } from "../core/app";
import { Button } from "../components/ui/button";
import { ComposerBox } from "./composer";
import { PromptCard } from "./prompt-card";

/** Below the transcript: the error banner, the agent's question, and the composer. */
export function ChatInput({ app, composer }: { app: App; composer: Composer }) {
  const { id, agent, state } = composer;
  return (
    <div className="chat-input shrink-0 px-6 pt-1 pb-4">
      <div className="mx-auto grid max-w-[820px] gap-2">
        {state.banner && <ChatBanner app={app} sessionId={id} banner={state.banner} />}
        {agent.prompt && <PromptCard app={app} sessionId={id} who={agent.who} prompt={agent.prompt} state={state} />}
        {!agent.unreachable && <RunChip.Pending app={app} sessionId={id} chair={agent} />}
        {/* An ended chair can pick its next run without starting it. */}
        {agent.who && agent.unreachable ? (
          <p className="text-xs text-muted-foreground" data-testid="composer-status">
            {agent.disabled}
          </p>
        ) : agent.unreachable ? (
          <div className="flex items-center justify-center gap-2">
            <RunChip app={app} sessionId={id} chair={agent} ended />
            <ResumeButton app={app} sessionId={id} native={agent.resumable} provider={agent.provider} />
          </div>
        ) : (
          <ComposerBox key={id} app={app} composer={composer} />
        )}
      </div>
    </div>
  );
}

function ResumeButton({ app, sessionId, native, provider }: { app: App; sessionId: string; native: boolean; provider: string | null }) {
  const snapshot = () => app.life.state;
  const state = useSyncExternalStore(app.life.subscribe, snapshot, snapshot);
  const run = state.endedRun?.[sessionId];
  const busy = state.resuming?.[sessionId] === true;
  if (!native && !(run?.model && run.provider !== provider)) return null;
  return (
    <Button size="sm" data-testid="resume-chat" disabled={busy} onClick={() => void app.life.resumeChat(sessionId)}>
      {busy ? "Resuming…" : "Resume"}
    </Button>
  );
}

function ChatBanner({ app, sessionId, banner }: { app: App; sessionId: string; banner: Banner }) {
  return (
    <div role="alert" data-testid="chat-banner" className="flex min-w-0 items-start gap-2 rounded-lg border border-failed/40 bg-failed/10 px-3 py-2 text-sm">
      <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-failed" />
      <span className="min-w-0 flex-1 font-mono text-xs leading-5 break-words whitespace-pre-wrap">{banner.text}</span>
      <Button variant="outline" size="sm" onClick={banner.retry}>
        Retry
      </Button>
      <Button variant="ghost" size="sm" onClick={() => app.dismissBanner(sessionId)}>
        Dismiss
      </Button>
    </div>
  );
}

