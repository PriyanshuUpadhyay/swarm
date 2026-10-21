# Step runner tests for the done gate: claim numbers, stale inputs, and script checks.
import re
import shlex

from test_step_run_support import *

CANVAS = ("`rg -n -g '*.html' -g '*.js' -g '*.mjs' -g '*.ts' -g '!**/vendor/**' -g '!**/node_modules/**' "
          "-g '!*.min.js' 'Math\\.random\\(|Date\\.now\\(' {site}` prints nothing: no render state in the piece's own "
          "code comes from an unseeded random or the wall clock.")


def gate_run(tmp, name, todos):
    """A two-step run (02 needs 01) whose 02 has these todo items, both steps checked with evidence."""
    folder = Path(tmp, name)
    skill = Path(tmp, f"{name}.md")
    skill.write_text("| File | Needs | Holds |\n|---|---|---|\n| 01-ask | none | the question |\n"
                     "| 02-work | ask | the work |\n", encoding="utf-8")
    assert run("start", folder, skill).returncode == 0
    for step, items in (("01-ask", []), ("02-work", todos)):
        file = folder / f"{step}.md"
        text = file.read_text(encoding="utf-8")
        text = re.sub(r"- \[ \] (.*): \n", r"- [x] \1: ok\n", text)
        text = text.replace(f"{step_run.RESULT_HEAD}\n", "".join(f"- [x] {i}: ok\n" for i in items)
                            + f"\n{step_run.RESULT_HEAD}\n", 1)
        file.write_text(text, encoding="utf-8")
    return folder


def take(folder, step, who="w"):
    out = run("take", folder, step, who)
    assert out.returncode == 0, out.stderr
    return out.stdout.splitlines()[0].rsplit(" #", 1)[1]


def test_done_needs_the_current_claim(tmp):
    """take numbers each claim, and done refuses an old number, so a worker replaced by --force cannot close."""
    folder = gate_run(tmp, "claim", [])
    assert take(folder, "01-ask") == "1" and take(folder, "01-ask") == "2"
    out = run("done", folder, "01-ask", "#1")
    assert out.returncode == 1 and "claim #1 is not the current one" in out.stderr and "active w #2" in out.stderr, out
    assert run("take", folder, "01-ask", "other").returncode == 1, "a claim by another name still needs --force"
    assert run("take", folder, "01-ask", "other", "--force").stdout.startswith("Status: active other #3")
    assert run("done", folder, "01-ask", "#2").returncode == 1
    out = run("done", folder, "01-ask", "3")
    assert out.returncode == 0, out.stderr
    assert run("done", folder, "01-ask", "#3").returncode == 1, "a done step has no claim to close"
    assert run("done", folder, "01-ask").returncode == 1, "done needs the claim number"


def test_done_refuses_a_stale_input(tmp):
    folder = gate_run(tmp, "stale", [])
    assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
    claim = take(folder, "02-work")
    ask = folder / "01-ask.md"
    ask.write_text(ask.read_text(encoding="utf-8") + "a later edit\n", encoding="utf-8")
    out = run("done", folder, "02-work", claim)
    assert out.returncode == 1 and "input changed after take: 01-ask" in out.stderr, out
    out = run("done", folder, "02-work", take(folder, "02-work"))
    assert out.returncode == 0, out.stderr


def test_done_runs_script_checks(tmp):
    """A box checked "ok" on a failing check is refused; the evidence text never runs; an edited command is refused."""
    site = Path(tmp, "piece")
    site.mkdir()
    (site / "index.js").write_text("const seed = Math.random();\n", encoding="utf-8")
    line = CANVAS.format(site=shlex.quote(str(site)))
    folder = gate_run(tmp, "checks", [f"block build-canvas script: {line}"])
    assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
    claim = take(folder, "02-work")
    out = run("done", folder, "02-work", claim)
    assert out.returncode == 1 and "block build-canvas check failed (exit 0, expected: prints nothing)" in out.stderr, out
    assert "Math.random" in out.stderr and str(site) in out.stderr, "the refusal shows the command and its output"
    assert "check build-canvas exit 0" in (folder / "events.log").read_text(encoding="utf-8")
    (site / "index.js").write_text("const seed = 7;\n", encoding="utf-8")
    out = run("done", folder, "02-work", claim)
    assert out.returncode == 0, out.stderr

    work = gate_run(tmp, "tamper", [f"block build-canvas script: `true` prints nothing: {line}"])
    assert run("done", work, "01-ask", take(work, "01-ask")).returncode == 0
    out = run("done", work, "02-work", take(work, "02-work"))
    assert out.returncode == 1 and "the check text differs from blocks/build-canvas" in out.stderr, out

    marker = Path(tmp, "ran")
    read = gate_run(tmp, "reader", ["block look-check script: `ls {{shots}}/compare-*.png | wc -l` prints 1 or more: "
                                    f"a compare image exists.: `touch {marker}`"])
    text = (read / "02-work.md").read_text(encoding="utf-8").replace("{{shots}}", shlex.quote(str(site)))
    (read / "02-work.md").write_text(text, encoding="utf-8")
    assert run("done", read, "01-ask", take(read, "01-ask")).returncode == 0
    out = run("done", read, "02-work", take(read, "02-work"))
    assert out.returncode == 0 and "not run: block look-check: its expectation needs a reader" in out.stdout, out
    assert not marker.exists(), "done never runs the worker's evidence"


def test_kit_check_lines(tmp):
    """Each kit `Check (script)` line is runnable or named as not run, and the runnable ones fill every placeholder."""
    import block_resolver
    runnable, reader = [], []
    for file in sorted((step_run.KIT / "blocks").glob("*/SKILL.md")):
        parts = block_resolver.sections(file.read_text(encoding="utf-8"))
        for line in block_resolver.check_lines(parts, "Check (script)"):
            (runnable if step_run.RUN_FORM.match(line) else reader).append(f"{file.parent.name}: {line[:40]}")
            assert any(p.match(line + ": ok") for p in step_run.block_checks(file.parent.name)), line
    assert len(runnable) == 10 and len(reader) == 10, (runnable, reader)
    # The kit's own test scripts run through done with <kit root> filled and pass.
    for block, script in (("plan-review", "test_start_plan.py"), ("comment-round", "test_comment_round.py")):
        line = next(l for l in block_resolver.check_lines(block_resolver.sections(
            (step_run.KIT / "blocks" / block / "SKILL.md").read_text(encoding="utf-8")), "Check (script)"))
        folder = gate_run(tmp, f"kit-{block}", [f"block {block} script: {line}"])
        assert run("done", folder, "01-ask", take(folder, "01-ask")).returncode == 0
        out = run("done", folder, "02-work", take(folder, "02-work"))
        assert out.returncode == 0, (block, out.stderr)
    review = gate_run(tmp, "kit-review", ["block change-review script: `sed -n 3p <repo-root>/tmp/review-check/"
                                          "<review-run>/03-verdict.md` starts with `Verdict: APPROVE`: the newest "
                                          "review approved the range."])
    assert run("done", review, "01-ask", take(review, "01-ask")).returncode == 0
    out = run("done", review, "02-work", take(review, "02-work"))
    assert out.returncode == 0 and "not run: block change-review: needs <review-run>" in out.stdout, out
