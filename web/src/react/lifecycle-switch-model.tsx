// Model switch progress, read from the phases the server sends.

import { CheckIcon, CircleIcon, LoaderCircleIcon, XIcon } from "lucide-react";
import { isSettled, type Handoff } from "../core/lifecycle";

export function SwitchProgress({ handoff }: { handoff: Handoff }) {
  const running = !isSettled(handoff.phase);
  const handoffText = handoff.carry === "file" || handoff.carry === "summary";
  const steps: [string, string][] = [
    ["preparing", handoff.carry === "file" ? "Write the chat text to a file" : "Read this chat"],
    ...(handoff.carry === "summary" ? [["summarizing", "Ask the agent for a summary"] as [string, string]] : []),
    ["starting", handoff.carry === "resume" ? "Restart this chat" : "Start the next CLI"],
    ...(handoffText ? [
      ["waiting", "Wait for the new agent"],
      ["delivering", handoff.carry === "file" ? "Send the file link" : "Send the summary"],
    ] as [string, string][] : []),
  ];
  const phaseIndex = steps.findIndex(([phase]) => phase === handoff.phase);
  return (
    <ol className="grid gap-1 text-sm" data-testid="sm-progress" data-phase={handoff.phase} aria-label="Progress" aria-live="polite">
      {steps.map(([phase, label], index) => {
        const now = phase === handoff.phase;
        const done = phaseIndex > index || handoff.phase === "completed";
        return (
          <li key={phase} className={`flex items-center gap-2 ${now ? "text-foreground" : "text-muted-foreground"}`}>
            {now && running ? <LoaderCircleIcon className="size-3.5 animate-spin motion-reduce:animate-none" /> : done ? <CheckIcon className="size-3.5 text-done" /> : <CircleIcon className="size-3.5" />}
            {label}
          </li>
        );
      })}
      {handoff.phase === "cancelled" && (
        <li className="flex items-center gap-2 text-muted-foreground">
          <XIcon className="size-3.5" /> Cancelled. This chat is unchanged.
        </li>
      )}
    </ol>
  );
}
