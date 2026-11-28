import { CircleAlertIcon, CircleCheckIcon, CircleIcon, OctagonXIcon } from "lucide-react";
import { createContext, useContext } from "react";
import { Glyph } from "./glyph";

import { shownStatus, type Status as WireStatus } from "../core/nav";

export const Offline = createContext(false);

export type Status = WireStatus | "idle";

const TITLE = {
  working: "Working",
  waiting: "Waiting",
  failed: "Failed",
  done: "Done",
  ended: "Ended",
};

const ICON = {
  waiting: CircleAlertIcon,
  failed: OctagonXIcon,
  done: CircleCheckIcon,
  ended: CircleIcon,
};

/** A shape and a status colour, so colour is never the only signal. `data-tip` feeds the list tooltip. `label` names the state in the reader's words when the status word does not fit. */
export function StatusGlyph({ status, label }: { status: Status; label?: string }) {
  const offline = useContext(Offline);
  const shown = shownStatus(status);
  if (offline && (shown === "working" || shown === "waiting")) return <Glyph form="unlit" size={12} className="glyph" />;
  if (shown === "idle") return null;
  const className = `glyph ${shown}`;
  const name = label ?? shown;
  const tip = label ?? TITLE[shown];
  if (shown === "working") return <Glyph form="working" size={12} className={className} label={name} data-tip={tip} />;
  const Icon = ICON[shown];
  return <Icon className={className} role="img" aria-label={name} data-tip={tip} />;
}
