#!/usr/bin/env python3
"""Own the stop of a file review outside any repo: the verdict line, the next round, and the round cap.

  file_review.py <run>            exit 0 only when file-review.md ends in `Verdict: APPROVE` and holds no `fix` row
  file_review.py <run> --close    after every seat wrote its rows, write the one verdict line
  file_review.py <run> --next     move file-review.md to file-review-<N>.md for the next round

A row is `| <file or claim> | pass, fix, or note | <evidence> |`. `--close` writes `Verdict: APPROVE` when no
`fix` row stays. With fix rows it exits 1: before the cap it asks for the fixes and `--next`; at the cap it
writes `Verdict: BLOCKED (<n> fix)` for the owner. The cap is 2 rounds, because in trial 5 the page did not
change after round 2 while the rounds went on to 4. One owner line `continue: file-review` in <run>/answers.md
allows one more round.
"""

import re
import sys
from pathlib import Path

CAP = 2


def rounds(run):
    """This round's number: one more than the moved files file-review-<N>.md."""
    return 1 + len([p for p in run.glob("file-review-*.md") if re.fullmatch(r"file-review-\d+\.md", p.name)])


def allowed(run):
    """The round cap, plus one when the owner wrote `continue: file-review` in answers.md."""
    answers = run / "answers.md"
    lines = answers.read_text(encoding="utf-8").splitlines() if answers.is_file() else []
    return CAP + ("continue: file-review" in (line.strip() for line in lines))


def read(run):
    """(fix rows, verdict lines, all lines) of file-review.md, or SystemExit when it is missing."""
    file = run / "file-review.md"
    if not file.is_file():
        raise SystemExit(f"no {file}; each seat writes its rows there")
    lines = file.read_text(encoding="utf-8").splitlines()
    fixes = []
    for n, line in enumerate(lines, 1):
        cells = [c.strip() for c in re.split(r"(?<!\\)\|", line)[1:-1]] if line.startswith("|") else []
        # A header row is the one right above a |---| line; its kind cell may list "fix, pass, or note".
        header = n < len(lines) and re.fullmatch(r"\|(\s*:?-+:?\s*\|)+", lines[n].strip())
        # A fix row with an unescaped pipe, or a reason after the word fix, still counts as a fix.
        if not header and len(cells) >= 2 and re.match(r"fix\b", cells[1].lower().strip("`*")):
            fixes.append(cells[0])
        if cells and len(cells) != 3:
            raise SystemExit(f"file-review.md line {n} has {len(cells)} cells, not 3; write a | in a cell as \\| and end the row with |")
    return fixes, [l for l in lines if l.startswith("Verdict:")], lines


def check(run):
    if (n := rounds(run)) > allowed(run):
        raise SystemExit(f"round {n} is past the cap of {CAP}; the owner adds `continue: file-review` to {run}/answers.md, or the open rows go to the owner")
    fixes, verdicts, lines = read(run)
    if len(verdicts) != 1 or not lines or lines[-1] != verdicts[0]:
        raise SystemExit("file-review.md needs one verdict line, as its last line; run file_review.py <run> --close")
    if verdicts[0] != "Verdict: APPROVE":
        raise SystemExit(f"{verdicts[0]}: open fix rows: {'; '.join(fixes) or 'none'}")
    if fixes:
        raise SystemExit(f"Verdict: APPROVE with {len(fixes)} open fix row(s): {'; '.join(fixes)}")
    return 0


def close(run):
    fixes, _, lines = read(run)
    body = [line for line in lines if not line.startswith("Verdict:")]
    while body and not body[-1].strip():
        body.pop()
    blocked = fixes and rounds(run) >= allowed(run)
    if fixes and not blocked:
        print(f"{len(fixes)} fix row(s) open in round {rounds(run)}: {'; '.join(fixes)}. Fix them, then run --next and review "
              "only the files changed since this round")
        return 1
    verdict = f"Verdict: BLOCKED ({len(fixes)} fix)" if blocked else "Verdict: APPROVE"
    (run / "file-review.md").write_text("\n".join(body + ["", verdict]) + "\n", encoding="utf-8")
    print(verdict + (". Bring the open rows to the owner; this is no approval" if blocked else ""))
    return 1 if blocked else 0


def next_round(run):
    n = rounds(run)
    if n + 1 > allowed(run):
        raise SystemExit(f"round {n + 1} is past the cap of {CAP}; run --close to give the open rows to the owner")
    read(run)
    (run / "file-review.md").rename(run / f"file-review-{n}.md")
    print(f"round {n + 1}: seats write a new file-review.md")
    return 0


def main(argv):
    if len(argv) not in (1, 2) or argv[1:] not in ([], ["--close"], ["--next"]):
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    run = Path(argv[0])
    return {"--close": close, "--next": next_round}.get(argv[1] if argv[1:] else "", check)(run)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
