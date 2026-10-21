# Step runner write-scope cases (council C3): done refuses a change outside the Writes of the steps beside it.
from test_step_run_support import *
import step_scope  # noqa: E402

PLAN = """| File | Needs | Holds | Writes |
|---|---|---|---|
| `01-a.md` | none | part a | `src/a/` |
| `02-b.md` | none | part b | `src/b/` |
| `03-end.md` | a, b | the end | none |
"""


def scope_repo(tmp):
    """A git repo with a plan run in tmp/run (ignored), both writing steps ready, and their todos checked."""
    repo = Path(tmp, "repo")
    (repo / "src").mkdir(parents=True)
    git = lambda *a: subprocess.run(["git", "-C", str(repo), *a], capture_output=True, text=True, check=True,
                                    env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    git("init", "-q")
    (repo / ".git" / "info" / "exclude").write_text("/tmp/\n", encoding="utf-8")
    (repo / "src" / "keep.ts").write_text("1\n", encoding="utf-8")
    git("add", "-A")
    git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "one")
    (repo / "plan.md").write_text(PLAN, encoding="utf-8")
    folder = repo / "tmp" / "run"
    assert run("start", folder, repo / "plan.md").returncode == 0
    for step in ("01-a", "02-b"):
        f = folder / f"{step}.md"
        f.write_text(f.read_text(encoding="utf-8").replace("- [ ] part a: ", "- [x] part a: done")
                     .replace("- [ ] part b: ", "- [x] part b: done"), encoding="utf-8")
    return repo, folder


def test_done_refuses_a_write_outside_scope(tmp):
    repo, folder = scope_repo(tmp)
    assert (folder / "01-a.md").read_text(encoding="utf-8").splitlines()[2] == "Writes: src/a/"
    (repo / "src" / "b").mkdir()
    (repo / "src" / "b" / "before.ts").write_text("made before the take\n", encoding="utf-8")
    assert run("take", folder, "01-a", "x").returncode == 0
    assert "\t01-a\tbase " in (folder / "events.log").read_text(encoding="utf-8")
    (repo / "src" / "a").mkdir()
    (repo / "src" / "a" / "y.ts").write_text("in scope\n", encoding="utf-8")
    (repo / "src" / "b" / "x.ts").write_text("out of scope\n", encoding="utf-8")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "it changed src/b/x.ts, outside the Writes of every step that ran beside it (src/a/" in out.stderr, out
    assert (folder / "01-a.md").read_text(encoding="utf-8").startswith("Status: active x #1")
    # While 02-b is active, its Writes count too: a parallel step's files cannot be told apart.
    assert run("take", folder, "02-b", "y").returncode == 0
    assert run("done", folder, "01-a", "1").returncode == 0
    (repo / "README.md").write_text("no step writes this\n", encoding="utf-8")
    out = run("done", folder, "02-b", "1")
    assert out.returncode == 1 and "it changed README.md" in out.stderr, out
    # The user's git settings stay; only the variables that tie git to a repo go.
    with patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": "/x", "GIT_DIR": "/y"}):
        env = step_scope.clean_env()
    assert env["GIT_CONFIG_GLOBAL"] == "/x" and "GIT_DIR" not in env
    # A caller's GIT_DIR does not point the diff at another repo.
    other = Path(tmp, "other")
    subprocess.run(["git", "init", "-q", str(other)], check=True)
    out = subprocess.run([sys.executable, SCRIPT, "done", folder, "02-b", "1"], capture_output=True, text=True,
                         env={**os.environ, "GIT_DIR": str(other / ".git")})
    assert out.returncode == 1 and "it changed README.md" in out.stderr, out
    (repo / "README.md").unlink()
    (repo / "src" / "keep.ts").write_text("2\n", encoding="utf-8")
    out = run("done", folder, "02-b", "1")
    assert out.returncode == 1 and "it changed src/keep.ts" in out.stderr, out
    (repo / "src" / "keep.ts").write_text("1\n", encoding="utf-8")
    assert run("done", folder, "02-b", "1").returncode == 0


def test_scope_skips_a_step_done_before_the_take(tmp):
    repo, folder = scope_repo(tmp)
    assert run("take", folder, "01-a", "x").returncode == 0 and run("done", folder, "01-a", "1").returncode == 0
    # An agent name that holds " failed: " is still a take.
    assert run("take", folder, "02-b", "y failed: z").returncode == 0
    (repo / "src" / "a").mkdir()
    (repo / "src" / "a" / "late.ts").write_text("01-a was done before this take\n", encoding="utf-8")
    out = run("done", folder, "02-b", "1")
    assert out.returncode == 1 and "it changed src/a/late.ts" in out.stderr, out
    # A step taken again after this take runs beside it once more.
    assert run("take", folder, "01-a", "x2", "--force").returncode == 0
    assert run("done", folder, "02-b", "1").returncode == 0


def test_scope_counts_a_step_done_while_take_reads_the_tree(tmp):
    repo, folder = scope_repo(tmp)
    assert run("take", folder, "01-a", "x").returncode == 0
    real = step_scope.work_tree

    def finish_01a(folder):
        now = real(folder)
        (repo / "src" / "a").mkdir()
        (repo / "src" / "a" / "x.ts").write_text("01-a ends while 02-b reads the tree\n", encoding="utf-8")
        assert run("done", folder, "01-a", "1").returncode == 0
        return now

    with patch.object(step_scope, "work_tree", finish_01a), redirect_stdout(StringIO()):
        step_run.take(folder, "02-b", "y")
    assert run("done", folder, "02-b", "1").returncode == 0
    # A run folder typed in other letter case names the same repo on a case-blind disk.
    upper = Path(str(folder).replace("/repo/", "/REPO/"))
    if upper.exists():
        assert run("take", upper, "02-b", "y", "--force").returncode == 0
        assert run("done", upper, "02-b", "2").returncode == 0

def test_scope_parallel_order_names_and_a_lost_base(tmp):
    repo, folder = scope_repo(tmp)
    assert run("take", folder, "01-a", "x").returncode == 0 and run("take", folder, "02-b", "y").returncode == 0
    for path in ("src/a/café.ts", "src/b/b.ts"):
        (repo / path).parent.mkdir(exist_ok=True)
        (repo / path).write_text("x\n", encoding="utf-8")
    # 02-b is done first; 01-a still sees its files as a parallel step's, and git does not quote the é.
    assert run("done", folder, "02-b", "1").returncode == 0
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 0, out
    # A base tree that git pruned fails the check closed.
    f = folder / "03-end.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] the end: ", "- [x] the end: done"), encoding="utf-8")
    assert run("take", folder, "03-end", "z").returncode == 0
    (repo / "README.md").write_text("new\n", encoding="utf-8")
    subprocess.run(["git", "-C", str(repo), "prune", "--expire=now"], check=True,
                   env={k: v for k, v in os.environ.items() if not k.startswith("GIT_")})
    out = run("done", folder, "03-end", "1")
    assert out.returncode == 1 and "scope check failed" in out.stderr, out


def test_a_step_does_not_change_blocks_toml(tmp):
    # Even inside its Writes: an [off] line there would turn off the step's own script checks.
    repo, folder = scope_repo(tmp)
    f = folder / "01-a.md"
    f.write_text(f.read_text(encoding="utf-8").replace("Writes: src/a/", "Writes: src/a/, .blocks.toml"), encoding="utf-8")
    assert run("take", folder, "01-a", "x").returncode == 0
    (repo / ".blocks.toml").write_text('[off]\nplan-review = "no"\n', encoding="utf-8")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "it changed .blocks.toml, which only the owner changes" in out.stderr, out
    # git add skips an ignored file, so the hash that take recorded catches it.
    (repo / ".git" / "info" / "exclude").write_text("/tmp/\n/.blocks.toml\n", encoding="utf-8")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "it changed .blocks.toml" in out.stderr, out
    assert step_scope.write_paths("none") == [] and step_scope.write_paths("`a/`, b") == ["a/", "b"]
    for cell, error in (("", "a Writes cell is empty"), ("none, a/", "stands alone")):
        try:
            step_scope.write_paths(cell)
        except SystemExit as stop:
            assert error in str(stop), stop
        else:
            raise AssertionError(cell)


def test_scope_fails_closed_in_a_repo(tmp):
    repo, folder = scope_repo(tmp)
    assert run("take", folder, "01-a", "x").returncode == 0
    real = step_scope.git
    broken = lambda root, *args, env=None: (subprocess.CompletedProcess(args, 128, "", "fatal: broken")
                                             if args[:1] == ("add",) else real(root, *args, env=env))
    with patch.object(step_scope, "git", broken):
        try:
            step_run.done(folder, "01-a", "1")
        except SystemExit as stop:
            assert "scope check failed: git add -A: fatal: broken" in str(stop), stop
        else:
            raise AssertionError("a failed git add passed the scope check")
    events = folder / "events.log"
    events.write_text("".join(l for l in events.read_text(encoding="utf-8").splitlines(keepends=True) if "\tbase " not in l),
                      encoding="utf-8")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "its take recorded no base; take it again" in out.stderr, out
    with events.open("a", encoding="utf-8") as log:
        log.write("2026-01-01T00:00:00\t01-a\tbase 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "no .blocks.toml hash; take it again" in out.stderr, out
    # A repo that git cannot read is not "no git".
    (repo / ".git" / "HEAD").write_text("garbage\n", encoding="utf-8")
    out = run("done", folder, "01-a", "1")
    assert out.returncode == 1 and "scope check failed: git " in out.stderr, out
    # A run folder in a nested repo that git passes over for the repo around it refuses too.
    outer = Path(tmp, "outer")
    subprocess.run(["git", "init", "-q", str(outer)], check=True, env=step_scope.clean_env())
    (outer / "inner" / ".git").mkdir(parents=True)
    try:
        step_scope.work_tree(outer / "inner" / "run")
    except SystemExit as stop:
        assert "not the repo at" in str(stop) and str(outer / "inner") in str(stop), stop
    else:
        raise AssertionError("git answered from the outer repo and the check passed")
    # An unreadable .blocks.toml stops the take before it claims, with no traceback. Root reads a mode-0 file.
    if os.geteuid() != 0:
        (repo / ".git" / "HEAD").write_text("ref: refs/heads/master\n", encoding="utf-8")
        (repo / ".git" / "info" / "exclude").write_text("/tmp/\n/.blocks.toml\n", encoding="utf-8")  # git skips it
        (repo / ".blocks.toml").write_text("", encoding="utf-8")
        (repo / ".blocks.toml").chmod(0)
        try:
            out = run("take", folder, "02-b", "y")
        finally:
            (repo / ".blocks.toml").chmod(0o644)
            (repo / ".blocks.toml").unlink()
        assert out.returncode == 1 and "cannot be read" in out.stderr and "Traceback" not in out.stderr, out
        assert (folder / "02-b.md").read_text(encoding="utf-8").startswith("Status: open")
        (repo / ".git" / "HEAD").write_text("garbage\n", encoding="utf-8")
    # A take that cannot read the repo leaves the step open, not claimed with no base.
    out = run("take", folder, "02-b", "y")
    assert out.returncode == 1 and "scope check failed" in out.stderr, out
    assert (folder / "02-b.md").read_text(encoding="utf-8").startswith("Status: open")


def test_scope_needs_git_and_a_writes_line(tmp):
    folder = Path(tmp, "run")
    Path(tmp, "plan.md").write_text(PLAN, encoding="utf-8")
    assert run("start", folder, Path(tmp, "plan.md")).returncode == 0
    f = folder / "01-a.md"
    f.write_text(f.read_text(encoding="utf-8").replace("- [ ] part a: ", "- [x] part a: done"), encoding="utf-8")
    with patch.object(step_scope, "work_tree", lambda folder: None), redirect_stdout(StringIO()) as out:
        step_run.take(folder, "01-a", "x")
        step_run.done(folder, "01-a", "1")
    assert "not run: scope: no git work tree" in out.getvalue(), out.getvalue()
    with patch.object(step_scope.subprocess, "run", side_effect=FileNotFoundError("git")):
        assert step_scope.work_tree(folder) is None
    assert (folder / "01-a.md").read_text(encoding="utf-8").startswith("Status: done ")
    assert step_scope.writes("Status: open\nUses: \n\n## Todo\n") is None
    assert step_scope.writes("Status: open\nUses: \nWrites: none\n\nx") == []


