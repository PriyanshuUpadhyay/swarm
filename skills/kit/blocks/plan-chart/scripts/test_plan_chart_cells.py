"""Run: python3 test_plan_chart_cells.py. Plan check cases for Block cells and step briefs; the shared fixtures
live in test_plan_chart.py."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))  # python3 -I does not add it
from test_plan_chart import PLAN, chart, ledger  # noqa: E402


def test_a_parent_in_a_block_cell_names_the_step_and_the_parent():
    """A parent id in a Block cell loads all its parts; the error says which step and parent pulled a part in."""
    text = PLAN.replace("| change-review, voice-reply |", "| change-review, voice-reply, voice |")
    out, folder = chart(text)
    assert out.returncode == 1 and "block voice-pr is used by a step but not Chosen" in out.stderr, out
    assert "step 05-reply loads it through its parent voice, which loads all its parts" in out.stderr, out.stderr
    assert not (folder / "plan.html").exists()


def test_an_implement_plan_needs_a_brief_for_each_middle_step():
    """An implement plan gives a brief to each step that is not optional, first, or last, as research does."""
    table = """| File | Needs | Holds | Writes | Block | Why |
|---|---|---|---|---|---|
| 01-frame | none | the claims | none | requirements | the claims say when it is done |
| 02-design (optional) | frame | skipped | none | decision-with-default | no UI |
| 03-build | frame, design | the commits | `src/` | checked-units | small checked units |
| 04-review | build | the verdict | none | change-review | new seats judge the commits |
| 05-close | review | the reply | none | done-when-evidence, voice-reply | the owner reads the result |
"""
    chosen = ["requirements", "decision-with-default", "checked-units", "change-review", "done-when-evidence",
              "voice", "voice-reply"]
    head = "# Plan: add a flag\n\nPreset: implement\n\nDone when: the flag works.\n\n- the flag lists ids\n\nEvidence:\n- a test\n\n"
    brief = "## Steps\n\n### 1 · Build it\n\nStep: `03-build`\n\n- Edit `src/a.rs:10`; test `t_flag` asserts the ids.\n- Result: one commit.\n\n"
    out, folder = chart(head + table + ledger(chosen))
    assert out.returncode == 1 and 'step 03-build has no brief under "## Steps"' in out.stderr, out
    review = "### 2 · Review it\n\nStep: `04-review`\n\n- New seats review the range.\n- Result: the verdict.\n\n"
    out, folder = chart(head + brief + review + table + ledger(chosen))
    assert out.returncode == 0, out.stderr



def test_seed_searches_skip_the_frame_and_an_unused_step():
    """The seed-search check skips a step that needs none or that no step uses, as before check_briefs."""
    import plan_rules
    text = ("## Steps\n\n### 1 · Web\n\nStep: `02-web`\n\n- Seed searches:\n  - `a b`\n  - `c d`\n- Result: rows.\n\n"
            "### 2 · Extra\n\nStep: `03-extra`\n\n- Result: more rows.\n")
    rows = [("01-frame", "none", "q", ["requirements"], "w", "none", False),
            ("02-web", "frame", "r", ["deep-search"], "w", "none", False),
            ("03-extra", "web", "r", ["deep-search"], "w", "none", False)]
    plan_rules.check_layout(text, rows, {"01-frame": [], "02-web": ["01-frame"], "03-extra": ["02-web"]})

if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
