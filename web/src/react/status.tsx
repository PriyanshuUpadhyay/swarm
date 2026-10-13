import { CircleAlertIcon, CircleCheckIcon, CircleIcon, LoaderCircleIcon, OctagonXIcon } from "lucide-react";

export type Status = "working" | "waiting" | "failed" | "done" | "ended" | "idle";

const TITLE: Record<Exclude<Status, "idle">, string> = {
  working: "Working",
  waiting: "Waiting for you",
  failed: "Failed",
  done: "Done",
  ended: "Ended",
};

const ICON = {
  working: LoaderCircleIcon,
  waiting: CircleAlertIcon,
  failed: OctagonXIcon,
  done: CircleCheckIcon,
  ended: CircleIcon,
};

/** A shape and a status colour, so colour is never the only signal. `data-tip` feeds the list tooltip. */
export function StatusGlyph({ status }: { status: Status }) {
  if (status === "idle") return null;
  const Icon = ICON[status];
  return <Icon className={`glyph ${status}`} role="img" aria-label={status} data-tip={TITLE[status]} />;
}
