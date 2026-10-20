"""Run: python3 test_implement_run.py  (uses a temporary repo and a temporary HOME). Needs Python 3.11 or later,
because start reads TOML with tomllib."""
import fcntl
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

if sys.version_info < (3, 11):
    raise SystemExit(f"test_implement_run.py needs Python 3.11 or later (tomllib); this is Python {sys.version.split()[0]}")

sys.path.insert(0, str(Path(__file__).resolve().parent))  # python3 -I does not add the script's folder

from test_implement_run_support import *
from test_implement_run_commands import *
from test_implement_run_callers import *
from test_implement_run_output import *
from test_implement_run_start import *
from test_implement_run_blocks import *
from test_implement_run_configuration import *
from test_implement_run_block_errors import *
from test_implement_run_block_metadata import *
from test_implement_run_visual_checks import *
from test_implement_run_archive import *


with tempfile.TemporaryDirectory() as tmp:
    home, repo = Path(tmp, "home"), Path(tmp, "app")
    (home / ".flow" / "app").mkdir(parents=True)
    rule = Path(tmp, "rule.md")
    rule.write_text("RULE TEXT 42\n")
    (home / ".flow" / "domains.md").write_text(f"## any\n- build: {rule}\n## web\n- build: {rule}, /nope.md\n")
    (home / ".flow" / "app" / "profile.md").write_text("Domains: any\nRepo rules: none\n")
    repo.mkdir()
    env = {**git_env(), "HOME": str(home)}
    run = lambda *a: subprocess.run([sys.executable, SCRIPT, *map(str, a)], cwd=repo, env=env, capture_output=True, text=True)
    git = lambda *a: subprocess.run(["git", *a], cwd=repo, env=env, capture_output=True, text=True, check=True).stdout.strip()
    git("init", "-q", "-b", "main")
    (repo / "app.py").write_text("def parse_phone(s):\n    return s\n\nprint(parse_phone('1'))\n")
    git("add", "app.py")
    git("commit", "-q", "-m", "one")

    before = set((repo / "tmp" / "flow").glob("*"))
    for invalid in ("", "bad\nkey"):
        rejected = run("start", "--parent-loop", invalid)
        assert rejected.returncode == 1 and "nonempty key" in rejected.stderr, rejected
        assert set((repo / "tmp" / "flow").glob("*")) == before, "invalid parent key left a half-made run"
    folder = Path(run("start").stdout.splitlines()[0])
    loop_key = json.loads((folder / "loop.json").read_text(encoding="utf-8"))["parent_loop"]
    assert loop_key.startswith("implement:")
    assert run("start").returncode == 0
    assert json.loads((folder / "loop.json").read_text(encoding="utf-8"))["parent_loop"] == loop_key
    assert run("start", "--parent-loop", "another-task").returncode == 1
    git("checkout", "-qb", "re-plan")
    replanned = Path(run("start", "--parent-loop", loop_key).stdout.splitlines()[0])
    assert replanned != folder
    assert json.loads((replanned / "loop.json").read_text(encoding="utf-8"))["parent_loop"] == loop_key
    git("checkout", "-q", "main")
    assert "RULE TEXT 42" in (folder / "05-build.md").read_text()
    assert "## Rules for this step (written by implement_run.py start; apply them, keep this section)" in (folder / "05-build.md").read_text()
    assert "RULE TEXT 42" not in (folder / "02-design.md").read_text()
    assert sorted(p.name for p in folder.glob("0*.md"))[0] == "01-frame.md"
    assert "Branch: main" in (folder / "01-frame.md").read_text()
    assert "Take with:" in (folder / "05-build.md").read_text()
    assert "/tmp/" in (repo / ".git" / "info" / "exclude").read_text()
    # The folder carries its step graph on each Uses line, and 05-build names its revision rule,
    # so a reader without this script (the Swarm app) can draw the graph and judge stale.
    uses = {p.stem: p.read_text().splitlines()[1] for p in folder.glob("0*.md")}
    assert uses["01-frame"] == "Uses:" and uses["04-impact"] == "Uses: 03-contracts", uses
    assert uses["05-build"] == "Uses: 04-impact, 02-design" and uses["07-close"] == "Uses: 06-review", uses
    assert "\nRevision: HEAD\n" in (folder / "05-build.md").read_text()
    assert "Revision:" not in (folder / "06-review.md").read_text()
    # A catalog with no block item writes no block, no profile, and no judge section (old behaviour).
    assert all("block:" not in text and "Judge checks" not in text
               and "profile " not in next(l for l in text.splitlines() if l.startswith("Skills read:"))
               for text in (p.read_text(encoding="utf-8") for p in folder.glob("0*.md")))

    test_close_from_other_cwd(tmp)
    test_second_same_day_close(tmp)
    test_close_archives_run(tmp)
    test_catalog_uses_running_kit(tmp)
    test_skip_text_encoding()
    test_kit_pointers()
    test_kit_block_frontmatter()
    test_command_wrong_count(run, folder)
    test_done_wrong_count(run, folder)
    test_take_force(run, folder)
    test_take_requires_who(run, folder)
    test_done_rejects_force(run, folder)
    test_closed_stdout(repo, env, folder)

    blocked = run("take", folder, "05-build", "a")
    assert blocked.returncode != 0 and "needs 04-impact, 02-design" in blocked.stderr

    # Only design, contracts, and impact can be skipped, and only with a reason.
    assert "cannot be skipped" in run("skip", folder, "05-build", "small").stderr
    assert "skip <folder> <step> <why>" in run("skip", folder, "02-design").stderr
    test_skip_plain_reason(run, folder)
    test_skip_lock(run, folder)
    test_skip_rejects_force(run, folder)
    test_atomic_skip(folder)
    assert run("skip", folder, "02-design", "no", "UI").returncode == 0
    assert (folder / "02-design.md").read_text().startswith("Status: skipped no UI\n")

    for step in ("01-frame", "03-contracts"):
        assert run("take", folder, step, "a").returncode == 0
        fill(folder / f"{step}.md")
        assert run("done", folder, step).returncode == 0, step
    c = folder / "03-contracts.md"
    c.write_text(c.read_text() + "`parse_phone()` rejects an empty string. `app.py` and `init` stay.\n")

    # take 04-impact writes the places that name each code name of the contracts.
    test_callers_after_claim(run, folder)
    test_impact_closed_stdout(repo, env, folder)
    test_impact_utf8(repo, env, folder)
    test_callers_timeout(folder)
    test_callers_long_stderr(folder)
    test_callers_stdout_utf8_cut(folder)
    test_callers_stderr_utf8_cut(folder)
    test_callers_large_output(folder)
    test_callers_long_line(folder)
    test_callers_failure(folder)
    test_callers_deadline(folder)
    test_callers_name_cap(folder)
    test_callers_below_rules(folder)
    test_callers_cut_list(folder)
    test_callers_end_only_in_impact(tmp)
    test_callers_controls(repo, run, folder)
    out = run("take", folder, "04-impact", "a")
    assert out.returncode == 0, out.stderr
    assert implement_run.CALLERS_HEAD in out.stdout, "take must print the complete impact file"
    assert out.stdout == (folder / "04-impact.md").read_text() + "\n"
    impact = (folder / "04-impact.md").read_text()
    assert "### parse_phone (2 places)" in impact and "app.py:1:" in impact and "### init" not in impact
    assert impact.index("## Callers") < impact.index("## Result")
    # Keep the old literal so a later heading rename still checks that existing runs resume.
    legacy_heading = "## Callers (written by implement_run.py take from the code names in 03-contracts.md; a search, not a judgment)"
    (folder / "04-impact.md").write_text(impact.replace(implement_run.CALLERS_HEAD, legacy_heading), encoding="utf-8")
    assert run("take", folder, "04-impact", "a").returncode == 0
    assert (folder / "04-impact.md").read_text(encoding="utf-8").count("## Callers (") == 1, "resume must keep the stored callers section"
    fill(folder / "04-impact.md")
    assert run("done", folder, "04-impact").returncode == 0

    out = run("take", folder, "05-build", "agent-b")
    assert out.returncode == 0 and "RULE TEXT 42" in out.stdout
    text = (folder / "05-build.md").read_text()
    assert text.startswith("Status: active agent-b") and f"Skills read: {rule}" in text and text.rstrip().endswith("## Result")
    assert "05-build: active agent-b" in run("status", folder).stdout
    refused = run("done", folder, "05-build")
    assert refused.returncode != 0 and "Failing check first" in refused.stderr
    f = folder / "05-build.md"
    f.write_text(f.read_text().replace("- [ ] Task file path and Base commit (pair or auto mode): ",
                                       "- [x] Task file path and Base commit (pair or auto mode): "))
    assert "no evidence" in run("done", folder, "05-build").stderr
    fill(f)
    head = git("rev-parse", "HEAD")
    assert run("done", folder, "05-build").returncode == 0 and f.read_text().startswith(f"Status: done {head[:12]}\n")

    # Close needs the review-check run's own APPROVE verdict on the current head.
    out = run("take", folder, "06-review", "a")
    assert out.returncode == 0 and "bring the open rows" not in out.stdout
    r = folder / "06-review.md"
    review_text = r.read_text()
    for count in (1, 2, 3):
        r.write_text(review_text + "".join(f"Run: tmp/review-check/range-{i}\n" for i in range(count)))
        out = run("take", folder, "06-review", "a")
        assert out.returncode == 0, out.stderr
        assert "bring the open rows" not in out.stdout, "review-check start owns the guard"
        assert r.read_text().startswith("Status: active a\n")
        assert [line for line in r.read_text().splitlines() if line.startswith("Run:")] == [
            f"Run: tmp/review-check/range-{i}" for i in range(count)], "take must keep one Run: line per round, newest last"
    r.write_text(review_text)
    review = Path(repo, "tmp", "review-check", "range-1")
    review.mkdir(parents=True)
    (review / "target.json").write_text(json.dumps({"head": head}))
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: REQUEST CHANGES (1 of 1 units; 1 fix)\n")
    fill(r)
    assert run("done", folder, "06-review").returncode == 0
    assert "needs a `Run:" in run("take", folder, "07-close", "a").stderr
    r.write_text(r.read_text() + "Run: tmp/review-check/range-1\n")
    assert "REQUEST CHANGES" in run("take", folder, "07-close", "a").stderr
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: NEEDS DISCUSSION (1 of 1 units; 1 ask)\n")
    assert "NEEDS DISCUSSION" in run("take", folder, "07-close", "a").stderr
    r.write_text(r.read_text() + "User accepted: ship it, the ask is a follow-up\n")
    refused = run("take", folder, "07-close", "a")
    assert refused.returncode != 0 and "NEEDS DISCUSSION" in refused.stderr, "User accepted: must not bypass the run's verdict"
    r.write_text(r.read_text().replace("User accepted: ship it, the ask is a follow-up\n", ""))
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: APPROVE (1 of 1 units; 0 fix)\n")

    # A new commit moves the build revision, so the review is stale and close refuses.
    (repo / "app.py").write_text((repo / "app.py").read_text() + "# fix\n")
    git("commit", "-q", "-am", "two")
    assert "06-review: done" in run("status", folder).stdout and "(stale: 05-build)" in run("status", folder).stdout
    assert "covers" in run("take", folder, "07-close", "a").stderr
    (review / "target.json").write_text(json.dumps({"head": git("rev-parse", "HEAD")}))
    assert run("take", folder, "07-close", "a").returncode == 0

    events = (folder / "events.log").read_text()
    assert "02-design\tskip no UI" in events and "07-close\ttake a" in events
    test_git_env_ignores_caller_git_dir(tmp)
    test_installed_kit_paths(tmp)
    test_start_block_ledger(tmp)
    test_start_domains(tmp)
    test_start_plan_needs_approval(tmp)
    test_auto_mode_needs_the_owners_words(tmp)
    test_start_ledger_without_chosen_blocks(tmp)
    test_block_in_step(tmp)
    test_block_hash_bytes(tmp)
    test_block_alias_and_base(tmp)
    test_block_profile(tmp)
    test_profile_on_second_start(tmp)
    test_block_example_params(tmp)
    test_block_params(tmp)
    test_block_controls(tmp)
    test_block_headings(tmp)
    test_skill_fenced_headings(tmp)
    test_start_without_tomllib(tmp)
    test_start_kit_without_blocks(tmp)
    test_toml_no_block_adds_no_base(tmp)
    test_no_toml_runs_blocks_without_base(tmp)
    test_param_quoted_in_script_checks(tmp)
    test_param_outside_span_in_script_check(tmp)
    test_add_path_errors(tmp)
    test_block_fail_closed(tmp)
    test_block_frontmatter(tmp)
    test_kit_blocks_load(tmp)
    test_parent_selection(tmp)
    test_parent_refusals(tmp)
    test_diagram_membership()
    test_shared_rules_not_copied(tmp)
    test_block_repo_file(tmp)
    test_add_folder_block(tmp)
    test_block_cap(tmp)
    test_block_checks(tmp)
    test_html_diagram_check(tmp)
    test_design_tokens_colors(tmp)
    test_design_tokens_page_code_colors(tmp)
    test_diagram_shot_check(tmp)
    test_design_tokens_dashes(tmp)
    test_web_ui_basics_checks(tmp)
    test_block_ascii_locale(tmp)
    test_review_read_encoding(tmp)
    print("ok")
