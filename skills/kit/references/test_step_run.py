"""Run: python3 test_step_run.py"""
import fcntl, os, subprocess, sys, tempfile, time
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))  # python3 -I does not add the script's folder
import step_run  # noqa: E402

from step_test_support import *
from step_test_commands import *
from step_test_todos import *
from step_test_build import *


def test_explicit_text_encoding():
    import ast
    for name in ("test_step_run.py", "step_test_support.py", "step_test_commands.py",
                 "step_test_todos.py", "step_test_build.py"):
        tree = ast.parse(Path(__file__).with_name(name).read_text(encoding="utf-8"))
        missing = [node.lineno for node in ast.walk(tree) if isinstance(node, ast.Call)
                   and isinstance(node.func, ast.Attribute) and node.func.attr in ("read_text", "write_text", "open")
                   and not any(kw.arg == "encoding" for kw in node.keywords)]
        assert not missing, f"text operations without UTF-8 encoding at {missing}"


test_table_blocks()
test_build_start_without_tomllib()
test_build_piece_repo_paths()
test_build_repo_additions()
test_block_command()
test_build_block_todos()
test_build_start_and_skip()


test_start_refuses_bad_needs()

with tempfile.TemporaryDirectory() as tmp:
    skill = Path(tmp, "SKILL.md")
    skill.write_text("| File | Needs | Holds |\n|---|---|---|\n| `01-ask.md` | none | the question |\n"
                     "| `02-look.md` | ask | what the sources say |\n\nafter\n", encoding="utf-8")
    folder = Path(tmp, "run")
    run("start", folder, skill)
    assert (folder / "02-look.md").read_text(encoding="utf-8").splitlines()[1] == "Uses: 01-ask"
    assert run("take", folder, "02-look", "a").returncode != 0
    assert (folder / "02-look.lock").exists()
    assert "the question" in run("done", folder, "01-ask").stderr
    f = folder / "01-ask.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] the question: ", "- [x] the question: why is the sky blue"), encoding="utf-8")
    assert run("done", folder, "01-ask").returncode == 0 and f.read_text(encoding="utf-8").startswith("Status: done ")
    test_explicit_text_encoding()
    test_command_wrong_count()
    test_plain_controls()
    test_done_wrong_count(folder)
    test_done_rejects_force(folder)
    test_lock_holder_failed_handshake(folder)
    test_process_lock(folder)
    test_dead_run_unlock(folder)
    test_no_lock_path_mutation(folder)
    test_claim_failure_log(folder)
    test_claim_failure_log_one_line(folder)
    test_step_mode(folder)
    test_old_todo_head(folder)
    test_rules_todo_heading(folder)
    test_result_todo_heading(folder)
    test_todo_head_pinned()
    test_todo_decoy_in_list(folder)
    test_todo_list_cut_names_line(folder)
    test_todo_list_shapes(folder)
    test_utf8_and_lock_context(folder)
    test_cli_output(folder)
    test_atomic_step_writes(folder)
    out = run("take", folder, "02-look", "agent-b")
    assert out.returncode == 0 and "Uses: 01-ask@" in (folder / "02-look.md").read_text(encoding="utf-8")
    assert "(ready)" not in run("status", folder).stdout.split("02-look")[1]
    assert "stale" not in run("status", folder).stdout
    f.write_text(f.read_text(encoding="utf-8") + "a later edit\n", encoding="utf-8")
    assert "02-look: active agent-b  (stale: 01-ask)" in run("status", folder).stdout
    events = [l.split("\t")[1:] for l in (folder / "events.log").read_text(encoding="utf-8").splitlines()]
    assert events[0][0] == "01-ask" and events[0][1].startswith("done ") and events[1] == ["02-look", "take agent-b"]
    claimed = folder / "02-look.md"
    original = claimed.read_text(encoding="utf-8")
    original_events = (folder / "events.log").read_text(encoding="utf-8")
    out = run("take", folder, "02-look", "agent-c")
    assert out.returncode != 0
    assert out.stderr.strip() == "02-look is active for agent-b; use --force if that run is gone"
    assert claimed.read_text(encoding="utf-8") == original
    assert (folder / "events.log").read_text(encoding="utf-8") == original_events
    assert (folder / "02-look.lock").exists()
    out = run("take", folder, "02-look", "agent-c", "--force")
    assert out.returncode == 0 and claimed.read_text(encoding="utf-8").startswith("Status: active agent-c\n")
    assert (folder / "02-look.lock").exists()
    assert run("take", folder, "02-look", "agent-c").returncode == 0
    lock = folder / "02-look.lock"
    claimed.write_text(original.replace("Status: active agent-b", "Status: open"), encoding="utf-8")
    children = [subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "02-look", who],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                for who in ("agent-d", "agent-e")]
    outputs = [child.communicate() for child in children]
    assert sorted(child.returncode for child in children) == [0, 1], outputs
    winner = ("agent-d", "agent-e")[next(i for i, child in enumerate(children) if child.returncode == 0)]
    assert claimed.read_text(encoding="utf-8").startswith(f"Status: active {winner}\n")
    assert lock.exists()
    print("ok")
