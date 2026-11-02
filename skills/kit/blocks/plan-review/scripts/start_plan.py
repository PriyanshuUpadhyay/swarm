#!/usr/bin/env python3
"""Start the step run of a plan only when its stop rule holds.

  start_plan.py <run>                  <run> holds plan.md, plan.html, and the rounds in plan-review/
  start_plan.py <run> --auto <file>    first record the owner's approval from their words in <file>, then start

A plan whose `Preset:` line names a `Stop: chat` preset, and whose File, Needs, Holds, and Block rows equal
that preset's (in the table that the start reads), starts with no round. Any other plan, also one that adds
a step, a block, or a decoy table, needs the newest review round to have approved plan.html as it is now, and plan.html to carry the hash of plan.md as
it is now, so an edit to plan.md after the round stops the start. A preset with a `Start:`
line runs its own step run, so this script makes no step files for it and prints that command.

`--auto` is for an owner who approved in words, not on the page. plan.md names the words on a line
`Auto approval: "<the owner's words>"`, and <file> holds the owner's message. It refuses when the words are
not in <file> exactly, or when plan.html is not drawn from plan.md as it is now. It writes a normal round
file for plan.html with the words, the file, and the plan.md hash, so the drift and edit refusals stay.

A run whose step files exist starts again only when each file still matches plan.md (see drift): the same
needs, holds, Writes and Optional lines, brief, and blocks, and no file without a row. A plan that adds a step
only makes the new file. Any other change refuses and asks for a new run folder that names this one.
"""

import hashlib
import json
import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(KIT / "references"), str(KIT / "blocks" / "comment-round" / "scripts"),
                str(KIT / "skills" / "swarm" / "scripts")]  # python3 -I adds none of them
import comment_round  # noqa: E402
from block_resolver import read_utf8  # noqa: E402
import preset  # noqa: E402
import step_brief  # noqa: E402
import step_run  # noqa: E402


def approved(run):
    """The preset's Start command or None, after the stop rule holds; else SystemExit."""
    run = Path(run)
    file = preset.of_plan(run / "plan.md")
    stop, start, _ = preset.section(file) if file else ("owner", None, [])
    if stop == "owner" or not preset.same_steps(run / "plan.md", file):
        comment_round.check(run / "plan-review", [str(run / "plan.html")])
        # The page carries the hash of the plan.md it was drawn from, so a kit update that redraws it differently
        # does not count as an edit, and an edit to plan.md after the round does.
        if not page_matches(run):
            raise SystemExit("plan.md changed after the owner approved plan.html; run plan-chart and start a new round")
    return start


def page_matches(run):
    """The plan.md hash when plan.html carries it, else None. The hash is of the text with LF line ends, as
    plan_page draws it, so a CRLF file matches its own page."""
    plan_sha = hashlib.sha256((run / "plan.md").read_text(encoding="utf-8").encode("utf-8")).hexdigest()
    page = run / "plan.html"
    drawn = re.search(r'<meta name="plan-md-sha256" content="(\w+)">', read_utf8(page)) if page.is_file() else None
    return plan_sha if drawn and drawn[1] == plan_sha else None


def auto(run, source):
    """Write the next plan-review round as approved, from the owner's words in source; else SystemExit."""
    run, source = Path(run), Path(source)
    words = re.search(r'^Auto approval: "(.+)"$', (run / "plan.md").read_text(encoding="utf-8"), re.M)
    if not words:
        raise SystemExit('plan.md has no line Auto approval: "<the owner\'s words>"')
    said = words[1].split()
    # Three whole words at least, so a short quote such as "ok" cannot match inside another word ("book").
    if len(said) < 3:
        raise SystemExit(f"the words {words[1]!r} are too short; quote at least 3 words of the owner's message")
    phrase = r"(?<!\w)" + r"\s+".join(map(re.escape, said)) + r"(?!\w)"
    if not source.is_file() or not re.search(phrase, read_utf8(source)):
        raise SystemExit(f"the words {words[1]!r} are not in {source}; quote the owner exactly, or run the plan-review round")
    if not (plan_sha := page_matches(run)):
        raise SystemExit("plan.html is not drawn from plan.md as it is now; run plan-chart first")
    folder = run / "plan-review"
    found = comment_round.rounds(folder) if folder.is_dir() else []
    if missing := comment_round.unanswered(folder, found):
        raise SystemExit(missing)
    folder.mkdir(exist_ok=True)
    n = found[-1][0] + 1 if found else 1
    result = {"decision": "approved", "auto": {"words": words[1], "file": str(source.resolve()), "plan_sha256": plan_sha}}
    data = {"files": comment_round.digests(folder, [str(run / "plan.html")]), "result": result}
    (folder / f"{n}.json").write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"round {n}: approved by the owner's words in {source}")


def drift(run):
    """Each existing step file whose needs, holds, writes, blocks, or brief differ from plan.md, and each one with no row.

    step_run.start skips a file that exists, so a re-approved plan would otherwise run the old file in silence.
    The check sits here, not in start, because step_amend calls start on purpose to add one file. Block ids are
    compared without their hash, as start resolves them for this repo, so a repo `[off]` block is off here too.
    A kit update alone does not refuse: a part that the kit adds, and a file block that the kit no longer has,
    are no change. Only a plan block or a repo `[add]` block missing from the file, or a file block that the kit
    still has and the plan no longer gives, refuses."""
    run = Path(run)
    if not (have := step_run.steps(run)):
        return []
    rows = step_run.table(run / "plan.md")
    uses = step_run.check_needs([r[:3] for r in rows])
    brief = step_brief.sections(run / "plan.md")
    # A resolver error (a block the kit no longer has, a broken .blocks.toml, Python before 3.11) stops the start
    # with its own reason, because without the resolve a changed or dropped block cannot be seen.
    try:
        new_ids = {name: [r.split("@")[0].removeprefix("block:") for r in read if r.startswith("block:")]
                   for name, (read, _, _) in step_run.resolve_start_blocks(run, rows).items()}
    except SystemExit as error:  # its reasons can quote keys from a cloned .blocks.toml
        raise SystemExit(step_run.plain(str(error.code))) from None
    found = [f"{name} (no row in plan.md)" for name in have if name not in uses]
    for name, _, holds, chosen, writes, optional in rows:
        if name not in have:
            continue
        try:
            lines = read_utf8(run / f"{name}.md").split("\n")
        except SystemExit as error:  # a file that is not UTF-8 stops in one line, as a damaged one does
            raise SystemExit(step_run.plain(str(error.code))) from None
        if len(lines) < 3 or "" not in lines[2:]:
            raise SystemExit(step_run.plain(f"{run / name}.md is not a step file (no Uses: line and blank line below it); "
                                            "restore it or start a new run folder"))
        if (was := step_run.needs(run, name)) != uses[name]:
            found.append(f"{name} (needs: was {', '.join(was) or 'none'}, now {', '.join(uses[name]) or 'none'})")
        blank = lines.index("", 2)
        head, rest = lines[2:blank], "\n".join(lines[blank + 1:])
        if "".join(f"{x}\n" for x in head) != (scope := step_run.step_scope.scope_lines(writes, optional)):
            now = scope.strip().replace("\n", "; ").removeprefix("Writes: ") or "none"
            found.append(f"{name} (writes: was {'; '.join(head).removeprefix('Writes: ') or 'none'}, now {now})")
        elif not any(rest.startswith(brief.get(name, "") + tail) for tail in (step_run.SKILLS_HEAD, step_run.TODO_MARK)):
            found.append(f"{name} (brief changed)")
        # As step_run finds it: by its prefix, so an older kit's wording still matches, and past rules text.
        at = step_run.todo_scan("\n".join(lines))[0]
        first = ("\n".join(lines)[at:].split("\n") + ["", ""])[1] if at >= 0 else ""
        # start writes Holds as the first todo. An open one is exact; a checked one holds evidence after the
        # colon, which can hold a colon too, so a Holds cell cut back at a colon is not seen there.
        if not (first.rstrip() == f"- [ ] {holds}:" or first.startswith(f"- [x] {holds}:")):
            found.append(f"{name} (holds: now {holds})")
        read = next((x.removeprefix(step_run.SKILLS_HEAD).split(", ") for x in lines if x.startswith(step_run.SKILLS_HEAD)), [])
        old = [r.split("@")[0].removeprefix("block:") for r in read if r.startswith("block:")]
        new = list(new_ids.get(name, []))
        # A new id that is a plan block, or no kit block (so a repo `[add]` gave it), refuses; a new kit part does not.
        if any(b not in old and (b in (chosen or []) or not (KIT / "blocks" / b / "SKILL.md").is_file()) for b in new) or \
                any(b not in new and (KIT / "blocks" / b / "SKILL.md").is_file() for b in old):
            found.append(f"{name} (blocks: was {', '.join(old) or 'none'}, now {', '.join(new) or 'none'})")
    return found


def main(argv):
    if len(argv) not in (1, 3) or len(argv) == 3 and argv[1] != "--auto":
        raise SystemExit(__doc__.strip().splitlines()[2].strip())
    run = Path(argv[0])
    if len(argv) == 3:
        auto(run, argv[2])
    if start := approved(run):
        print(f"start with: {start}")
        return
    if changed := drift(run):
        # A repo's .blocks.toml can turn a block on or off too, which changes a step's rules as a plan edit does.
        raise SystemExit(f"plan.md or this repo's .blocks.toml changed after these steps were made: {step_run.plain('; '.join(changed))}. "
                         f"Start the new plan in a new run folder, such as {run}-2.")
    step_run.start(run, run / "plan.md")


if __name__ == "__main__":
    main(sys.argv[1:])
