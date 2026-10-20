# Rule catalogs, seat ownership, build gates, and CLI checks.
from test_review_check_support import *


def test_build_gate():
    """The repo's lint config decides which rule IDs the build owns, and the verdict counts them only from
    build.json for this head."""
    rules = Path.home() / ".review-check" / "rules" / f"{Path.cwd().name}.md"
    rules.parent.mkdir(parents=True, exist_ok=True)
    rules.write_text("Files: `**/*.ts`\n\nCI: `Lint` runs `true`; config `.oxlintrc.json`\n")
    config = {"ignorePatterns": ["vendor"], "rules": {
        "@typescript-eslint/no-floating-promises": "error", "@typescript-eslint/await-thenable": "error",
        "no-fallthrough": ["error", {"commentPattern": "skip"}]}}  # options: the fixtures do not speak for it
    base = commit({".oxlintrc.json": json.dumps(config), "a.ts": "const m = new Map();\nexport const v = m.get(1)!;\n",
                   "vendor/b.ts": "export {};\n"}, "lint base")
    head = commit({"a.ts": "const m = new Map();\nexport const v = m.get(1)!;\nexport async function f() { await g(); }\n",
                   "vendor/b.ts": "export async function f() { await g(); }\n"}, "lint change")
    rc.start(f"{base}..{head}")
    run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
    units = {u["file"]: u["id"] for u in json.loads((run / "units.json").read_text())}
    checklist = load_checklist(run)
    a, v = checklist[units["a.ts"]], checklist[units["vendor/b.ts"]]
    assert set(a["tool"]) == {"TS-1", "TS-24"} and not {"TS-1", "TS-24"} & set(a["rules"]), a
    assert "TS-6" not in a["rules"], "Scope: added, and the `!` is on a line the change did not add"
    assert not v["tool"] and "TS-24" in v["rules"], "an ignored path keeps its IDs with the seats"

    def answers(*extra):
        rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `f()` | n/a | | the test code has no such case |"
                    for uid, c in checklist.items()), *extra)

    answers()
    assert rc.verdict(run.name) == 1, "no build.json"
    (run / "build.json").write_text(json.dumps({"head": base, "digest": None, "source": "check-run", "name": "Lint",
                                                "conclusion": "success", "tail": []}))
    assert rc.verdict(run.name) == 1, "a build result for another head"
    (run / "build.json").write_text(json.dumps({"head": head, "digest": None, "source": "check-run", "name": "Lint",
                                                "conclusion": "success", "tail": []}))
    answers(f"| {units['a.ts']} | TS-24 | a.ts:3 | `await g()` | pass | | g returns a promise |")
    assert rc.verdict(run.name) == 1, "a seat row for an ID the build owns"
    answers()
    assert rc.verdict(run.name) == 0
    assert (run / "03-verdict.md").read_text().splitlines()[2].startswith("Verdict: APPROVE"), "build pass counts tool IDs"
    Path("a.ts").write_text("dirty\n")
    rc.build(run.name, "--run")  # the live tree can move while the frozen head is checked
    verify = Path("tmp/review-check/verify").resolve()
    assert sh("git", "-C", str(verify), "rev-parse", "HEAD") == head
    assert (verify / "a.ts").read_text() != "dirty\n"
    assert Path("a.ts").read_text() == "dirty\n", "build changed the live tree"
    sh("git", "checkout", "--", "a.ts")
    rc.build(run.name, "--run")  # the persistent worktree is reused
    assert json.loads((run / "build.json").read_text())["conclusion"] == "success"
    (run / "build.json").write_text(json.dumps({"head": head, "digest": None, "source": "command", "name": "true",
                                                "conclusion": "failure", "tail": ["a.ts(3,30): error TS2304"]}))
    assert rc.verdict(run.name) == 0
    line3 = (run / "03-verdict.md").read_text().splitlines()[2]
    assert line3.startswith("Verdict: REQUEST CHANGES") and line3.endswith("build: fail)"), line3



def test_shards():
    catalog = [{"id": f"C-{n}", "source": "lens.md", "aspect": "lens"} for n in range(1, 501)]
    catalog += [{"id": "PY-1", "source": "lens-python.md", "aspect": "lang"},
                {"id": "T-1", "source": "project.md", "aspect": "repo"}]
    checklist = {f"u{i}": {"rules": [f"C-{n}" for n in range(1, size + 1)], "tool": [], "refs": {}}
                 for i, size in enumerate([80, 70, 70, 70, 70, 70, 70], 1)}
    checklist["u1"]["rules"] += ["PY-1", "T-1", "REF"]
    seats = rc.seat_map(checklist, catalog)
    shards = [p for seat, p in seats.items() if seat.startswith("lens-")]
    assert len(shards) == 3 and sum(p["checks"] for p in shards) == 500, seats
    assert [uid for p in shards for uid in p["units"]] == list(checklist), "units split or out of order"
    assert max(p["checks"] for p in shards) == 210, "whole-unit ranges are not balanced"
    for uid, entry in checklist.items():
        assert set(entry["owners"]) == set(entry["rules"]), "a pair has no owner"
        for rule, owner in entry["owners"].items():
            assert uid in seats[owner]["units"]
            aspect = "lens" if rule.startswith("C-") else {"PY-1": "lang", "T-1": "repo", "REF": "refs"}[rule]
            assert sum(uid in p["units"] for seat, p in seats.items() if seat.split("-")[0] == aspect) == 1
    assert seats["refs"]["route"] == "review.deep", seats
    small = {"u1": {"rules": [f"C-{n}" for n in range(1, 101)]}}
    assert list(rc.seat_map(small, catalog)) == ["lens"], "a single shard must keep its old name"
    large_unit = {"u1": {"rules": [f"C-{n}" for n in range(1, 501)]}}
    assert list(rc.seat_map(large_unit, catalog)) == ["lens"], "a whole unit cannot be split"



def test_repo_rules_origin():
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as folder:
        try:
            repo = Path(folder) / "lens-api"
            repo.mkdir()
            os.chdir(repo)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"value.py": "value = 1\n"}, "base")
            head = commit({"value.py": "value = 2\n"}, "head")
            rules = Path.home() / ".review-check" / "rules" / "lens-api.md"
            rules.parent.mkdir(parents=True, exist_ok=True)
            rules.write_text("Files: `**/*.py`\n- `REPO-901` a project rule\n")
            rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            unit = next(iter(checklist.values()))
            assert unit["owners"]["REPO-901"] == "repo", unit
            seats = json.loads((run / "checklist.json").read_text())["seats"]
            assert seats["repo"]["checks"] == 1 and seats["repo"]["route"] == "review.check"
        finally:
            os.chdir(previous)



def test_seat_gaps():
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as folder:
        try:
            os.chdir(folder)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "value = 1\n", "README.md": "old docs\n"}, "base")
            head = commit({"util.py": "value = 2\n", "README.md": "new docs\n"}, "head")
            catalog = [{"id": f"C-{n}", "source": "lens.md", "aspect": "lens", "files": ["**/*.py"], "applies": None,
                        "check": [], "scope": None} for n in range(1, 6)]
            with patch.object(rc, "load_rules", return_value=catalog):
                rc.start(f"{base}..{head}")
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            units = {u["file"]: u["id"] for u in json.loads((run / "units.json").read_text())}
            uid, docs = units["util.py"], units["README.md"]
            # Keep this test small, with two named owners and one foreign answer.
            checklist[uid]["rules"] = ["C-1", "PY-3"]
            checklist[uid]["owners"] = {"C-1": "lens-1", "PY-3": "lang"}
            seats = {"lens-1": {"units": [uid], "checks": 1}, "lang": {"units": [uid], "checks": 1}}
            (run / "checklist.json").write_text(json.dumps(checklist | {"seats": seats}))
            row = f"| {uid} | PY-3 | util.py:1 | `value = 2` | pass | | the value has no caller |\n"
            (run / "02-review-lens-1.md").write_text(row)
            # A stray seat cannot fill a named owner's gap. The chair covers a unit with no rules.
            (run / "02-review-stray.md").write_text(row)
            rows(run, f"| {docs} | - | README.md:1 | `new docs` | pass | | this unit only changes prose |")
            with redirect_stdout(StringIO()) as out:
                assert rc.verdict(run.name) == 1
            text = out.getvalue()
            assert "seat lens-1:" in text and "checks owned by another seat" in text, text
            assert f"seat lens-1: {uid}: no result for C-1" in text, text
            assert f"seat lang: {uid}: no result for PY-3" in text, text
            assert text.index("no result for C-1") < text.index("no result for PY-3"), text
            (run / "02-review-lens-1.md").write_text(row.replace("PY-3", "C-1"))
            chair = run / "02-review.md"
            docs_row = chair.read_text()
            chair.write_text(docs_row + row)
            with redirect_stdout(StringIO()) as out:
                assert rc.verdict(run.name) == 1, "a chair row cannot replace the lang seat's answer"
            text = out.getvalue()
            assert "chair row names PY-3 owned by seat lang" in text, text
            assert f"seat lang: {uid}: no result for PY-3" in text, text
            assert "Verdict: APPROVE" not in (run / "03-verdict.md").read_text()
            chair.write_text(docs_row)
            (run / "02-review-lang.md").write_text(row)
            with redirect_stdout(StringIO()):
                assert rc.verdict(run.name) == 0
            assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()
            (run / "02-review-lens-1.md").write_text(row.replace("PY-3", "C-1, L-13"), encoding="utf-8")
            with redirect_stdout(StringIO()) as out:
                assert rc.verdict(run.name) == 1, "an unknown ID must keep the verdict incomplete"
            text = out.getvalue()
            assert "L-13 is not in this unit's checklist" in text, text
            assert "checks owned by another seat" not in text, text
            assert "no result for C-1" not in text and "no result for PY-3" not in text, text
        finally:
            os.chdir(previous)



def test_patch_errors():
    script = HERE / "review_check.py"
    for args, message in ((["--patch"], "--patch needs a file"),
                          (["--patch", "missing.patch"], "missing.patch")):
        result = subprocess.run([sys.executable, script, "start", "local", *args],
                                capture_output=True, text=True)
        assert result.returncode == 1, result
        assert message in result.stderr and "Traceback" not in result.stderr, result.stderr
        assert len(result.stderr.splitlines()) == 1, result.stderr



def test_cli_encoding():
    """An ASCII console escapes Unicode rows without changing the verdict or exit code."""
    previous = Path.cwd()
    with tempfile.TemporaryDirectory() as d:
        try:
            os.chdir(d)
            sh("git", "init", "-qb", "main")
            sh("git", "config", "user.email", "t@t")
            sh("git", "config", "user.name", "t")
            base = commit({"util.py": "def helper():\n    return 1\n"}, "console base")
            head = commit({"util.py": "def helper():\n    return '合意'\n"}, "console head")
            target = f"{base}..{head}"
            rc.start(target)
            run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
            checklist = load_checklist(run)
            rows(run, *(f"| {uid} | {', '.join(c['rules']) or '-'} | x | `return '合意'` | n/a | | "
                        "the test code has no case these rules name |" for uid, c in checklist.items()),
                 "| u1 | - | util.py:2 | `return '合意'` | note | helper → 合意 | caller logs the helper result |")
            script = str(HERE / "review_check.py")
            with TextIOWrapper(BytesIO(), encoding="ascii") as output:
                with redirect_stdout(output), patch.object(sys, "argv", [script, "verdict", run.name]):
                    try:
                        runpy.run_path(script, run_name="__main__")
                        raise AssertionError("the CLI must exit with its verdict result")
                    except SystemExit as e:
                        assert e.code == 0, e
                output.flush()
                assert "helper \\u2192 \\u5408\\u610f" in output.buffer.getvalue().decode("ascii")
            verdict = (run / "03-verdict.md").read_text(encoding="utf-8")
            assert "Verdict: APPROVE" in verdict and "helper → 合意" in verdict
            output = StringIO()
            with redirect_stdout(output), patch.object(sys, "argv", [script, "verdict", run.name]):
                try:
                    runpy.run_path(script, run_name="__main__")
                    raise AssertionError("the CLI must exit with its verdict result")
                except SystemExit as e:
                    assert e.code == 0, e
            assert "helper → 合意" in output.getvalue()

            env = dict(os.environ, LC_ALL="C", PYTHONUTF8="0", PYTHONCOERCECLOCALE="0", PYTHONIOENCODING="ascii")
            result = subprocess.run([sys.executable, script, "start", target], env=env,
                                    capture_output=True, encoding="utf-8", errors="replace")
            assert result.returncode == 0, result.stderr
            next_run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-02"
            assert "return '合意'" in (next_run / "head/util.py").read_text(encoding="utf-8")
        finally:
            os.chdir(previous)



def test_rust_function_range():
    """universal-ctags gives a Rust fn no end line, so the unit must still span the whole body,
    or a pass row that quotes a body line fails its quote check."""
    with tempfile.TemporaryDirectory() as d:
        src = Path(d) / "notice.rs"
        src.write_text('fn waiting_notice(new: &str) -> Option<String> {\n'
                       '    if new != "waiting" {\n        return None;\n    }\n'
                       '    Some(format!("{} needs you", new))\n}\n\nfn other() {}\n')
        assert ("waiting_notice", 1, 6) in rc.functions(src), rc.functions(src)



def test_catalog():
    """Rule IDs are unique, every regex compiles, and every `Check:` has bad, good, and exception fixtures."""
    catalog = rc.load_rules(sorted((HERE.parent / "references").glob("lens*.md")))
    ids = [r["id"] for r in catalog]
    assert len(ids) == len(set(ids)), sorted(i for i in ids if ids.count(i) > 1)
    for r in catalog:
        assert r["scope"] in (None, "added"), r["id"]
        for c in r["check"]:
            tool, _, name = c.partition(":")
            fixtures = HERE.parent / "fixtures" / tool / name
            assert tool == "oxlint" and all(list(fixtures.glob(f"{k}.*")) for k in ("bad", "good", "exception")), c



def test_fixtures():
    """Each `Check:` rule hits every `// expect` line of bad.* and nothing in good.* or exception.*, with the
    oxlint on PATH (for a repo's own version: PATH=<repo>/node_modules/.bin:$PATH)."""
    if not shutil.which("oxlint"):
        print("fixtures skipped: oxlint is not on PATH")
        return
    root = HERE.parent / "fixtures" / "oxlint"
    for r in rc.load_rules(sorted((HERE.parent / "references").glob("lens*.md"))):
        for c in r["check"]:
            rule = c.partition(":")[2]
            for f in sorted((root / rule).iterdir()):
                out = subprocess.run(["oxlint", "--type-aware", "-A", "all", "-D", rule, "-f", "json", str(f)],
                                     cwd=root, capture_output=True, text=True).stdout
                hits = {d["labels"][0]["span"]["line"] for d in json.loads(out)["diagnostics"]}
                want = {i for i, l in enumerate(f.read_text().splitlines(), 1) if "// expect" in l}
                assert hits == want, (r["id"], f.name, hits, want)

