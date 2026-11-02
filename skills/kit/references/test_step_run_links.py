# Step files link their block rules by hash; take prints them and refuses a changed block (A9, kit-right-size).
import re

from test_step_run_support import *


def test_linked_rules(tmp):
    """The file holds a link, not the rules; take prints the rules; an edited block refuses take; judge todos are unique."""
    repo = Path(tmp, "links")
    repo.mkdir()
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    subprocess.run(["git", "init", "-q", str(repo)], env=env, check=True)
    (repo / "docs").mkdir()
    block = repo / "docs" / "visual.md"
    block.write_text("## Rules\n- LINKED RULE TEXT\n## Check (script)\n- none: text only\n## Check (judge)\n"
                     "- Is it visual?\n- Is it visual?\n", encoding="utf-8")
    (repo / ".blocks.toml").write_text('[params]\nsource_of_truth = "docs/visual.md"\n[add]\n02-work = ["docs/visual.md"]\n',
                                       encoding="utf-8")
    folder = repo / "piece" / "ink"
    folder.mkdir(parents=True)
    (folder / "plan.md").write_text("| File | Needs | Holds | Writes | Block | Why |\n|---|---|---|---|---|---|\n"
                                    "| `01-ask.md` | none | the ask | none | requirements | it frames |\n"
                                    "| `02-work.md` | ask | the work | none | voice-reply | it replies |\n", encoding="utf-8")
    out = subprocess.run([sys.executable, SCRIPT, "start", folder, folder / "plan.md"], cwd=repo, env=env,
                         capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    text = (folder / "02-work.md").read_text(encoding="utf-8")
    assert "LINKED RULE TEXT" not in text and f"Rules from: {(folder / 'plan.md').resolve()}" in text, text
    assert text.count("judge: Is it visual?") == 1, "a repeated judge question is one todo"
    ask = folder / "01-ask.md"
    ask.write_text(re.sub(r"- \[ \] (.*): \n", r"- [x] \1: ok\n", ask.read_text(encoding="utf-8")), encoding="utf-8")
    assert run("take", folder, "01-ask", "w").returncode == 0 and run("done", folder, "01-ask", "1").returncode == 0
    out = run("take", folder, "02-work", "w")
    assert out.returncode == 0 and "LINKED RULE TEXT" in out.stdout and out.stdout.index("Rules from: ") < out.stdout.index("LINKED"), out
    block.write_text(block.read_text(encoding="utf-8").replace("LINKED RULE TEXT", "NEW RULE"), encoding="utf-8")
    out = run("take", folder, "02-work", "w")
    assert out.returncode == 1 and "its blocks changed after start" in out.stderr, out
    assert (folder / "02-work.md").read_text(encoding="utf-8").startswith("Status: active w #1"), "a refused take claims nothing"
    (folder / "plan.md").rename(folder / "plan-gone.md")
    out = run("take", folder, "02-work", "w")
    assert out.returncode == 1 and "which is gone" in out.stderr, out
