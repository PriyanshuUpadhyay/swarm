import { CheckIcon, XIcon } from "lucide-react";
import { plural } from "../core/lifecycle-state";
import type { LineTotals, PrFact } from "../core/nav";

export function PrChip({ pr, tabIndex }: { pr: PrFact | null | undefined; tabIndex?: number }) {
  if (!pr || !Number.isSafeInteger(pr.number) || pr.number < 1) return null;
  if (pr.state !== "open" && pr.state !== "draft" && pr.state !== "merged") return null;
  const url = URL.canParse(pr.url) ? new URL(pr.url) : null;
  if (!url || url.origin !== "https://github.com" || url.username || url.password || url.search || url.hash ||
    !/^\/[^/]+\/[^/]+\/pull\/\d+$/.test(url.pathname) || !url.pathname.endsWith(`/pull/${pr.number}`)) return null;

  const ci = pr.state === "merged" ? undefined : pr.ci;
  const stateLabel = ci === "passed" || ci === "failed" || ci === "pending" ? `${pr.state}, CI ${ci}` : pr.state;

  return (
    <a className="pr-chip" data-state={pr.state} data-testid="pr-chip" href={url.href} target="_blank" rel="noopener noreferrer" tabIndex={tabIndex}
      aria-label={`PR #${pr.number}, ${stateLabel}`} title={`PR #${pr.number}, ${stateLabel}`} onClick={(e) => e.stopPropagation()}>
      {ci === "passed" ? <CheckIcon className="pr-ci text-done" aria-hidden /> :
        ci === "failed" ? <XIcon className="pr-ci text-failed" aria-hidden /> :
        ci === "pending" ? <span className="pr-ci pr-pending" aria-hidden /> : null}
      <span>#{pr.number}{pr.state === "merged" ? " merged" : ""}</span>
    </a>
  );
}

/** Null means there was no stable comparison; a real zero has its own display. */
export function BranchDiff({ diff }: { diff: LineTotals | null | undefined }) {
  if (!diff || ![diff.addedLines, diff.removedLines].every((n) => Number.isSafeInteger(n) && n >= 0)) return null;
  return (
    <span className="branch-diff" aria-label={`${plural(diff.addedLines, "line")} added, ${plural(diff.removedLines, "line")} removed`}>
      <span className="text-done">+{diff.addedLines}</span>
      <span className="text-failed">−{diff.removedLines}</span>
    </span>
  );
}
