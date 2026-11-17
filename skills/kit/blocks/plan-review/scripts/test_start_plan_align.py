"""Align owner-gate cases; test_start_plan.py runs them."""
import subprocess, sys, tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("start_plan.py")

ALIGN = """## Align

| Required answer | Answer | Owner message |
|---|---|---|
| A1: empty input | Reject it (recommendation accepted by yes). | chat message 12 |
| A2: maximum length | none | chat message 12 |
| Acceptance | Start with an empty name; submit it; see a required-name error. | chat message 13 |
| Not built | Do not change saved names. | chat message 13 |

"""


def test_answered_align_starts_after_approval(plan, approve, start):
    with tempfile.TemporaryDirectory() as folder:
        run = Path(folder)
        (run / "plan.md").write_text(plan + "\n" + ALIGN, encoding="utf-8")
        approve(run)
        out = start(run)
        assert out.returncode == 0, out.stderr
        assert (run / "01-reply.md").read_text().startswith("Status: open\n")


def test_unanswered_align_refuses_before_writing(plan, approve):
    for answer, source in (("", "chat message 12"), ("skipped", "chat message 12"), ("Reject it.", ""), ("Reject it.", "none")):
        for in_words in (False, True):
            with tempfile.TemporaryDirectory() as folder:
                run = Path(folder)
                align = ALIGN.replace("Reject it (recommendation accepted by yes).", answer).replace("chat message 12", source)
                words = "Approve this plan now"
                text = plan + "\n" + align
                if in_words:
                    text = text.replace("Done when:", f'Auto approval: "{words}"\n\nDone when:')
                    (run / "owner.txt").write_text(words, encoding="utf-8")
                (run / "plan.md").write_text(text, encoding="utf-8")
                approve(run)
                rounds = list((run / "plan-review").iterdir())
                args = ["--auto", run / "owner.txt"] if in_words else []
                out = subprocess.run([sys.executable, "-I", SCRIPT, run, *args], capture_output=True, text=True)
                assert out.returncode == 1, (answer, source, out.stdout, out.stderr)
                assert not (run / "01-reply.md").exists()
                assert list((run / "plan-review").iterdir()) == rounds


