# Implement test output cases and helpers.
from test_implement_run_support import *


def test_impact_closed_stdout(repo, env, folder):
    file = folder / "04-impact.md"
    original = file.read_text()
    file.write_text(original + "large output\n" * 10000)
    try:
        with subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "04-impact", "pipe"],
                              cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            with subprocess.Popen(["head", "-1"], stdin=child.stdout, stdout=subprocess.PIPE) as consumer:
                child.stdout.close()
                output, _ = consumer.communicate()
                assert consumer.returncode == 0 and output.startswith(b"Status: active pipe #")
            errors = child.stderr.read()
            assert child.wait() == 0 and not errors, errors
        assert implement_run.CALLERS_HEAD in file.read_text(), "callers must be saved before printing to a closed pipe"
    finally:
        file.write_text(original)


def test_impact_utf8(repo, env, folder):
    file = folder / "04-impact.md"
    contracts = folder / "03-contracts.md"
    original = file.read_text(encoding="utf-8")
    original_contracts = contracts.read_text(encoding="utf-8")
    app = repo / "app.py"
    original_app = app.read_bytes()
    # Disable C-locale coercion, UTF-8 mode, and UTF-8 stream output independently.
    ascii_env = {**env, "LC_ALL": "C", "PYTHONUTF8": "0", "PYTHONCOERCECLOCALE": "0", "PYTHONIOENCODING": "ascii"}
    try:
        app.write_bytes(original_app + "# 合意 parse_phone\n".encode("utf-8") + b"# \xff parse_phone\n")
        for note in ("", "合意\n"):
            file.write_text(original, encoding="utf-8")
            contracts.write_text(original_contracts + note, encoding="utf-8")
            out = subprocess.run([sys.executable, SCRIPT, "take", str(folder), "04-impact", "ascii-owner"],
                                 cwd=repo, env=ascii_env, capture_output=True, text=True)
            assert out.returncode == 0, out.stderr
            assert "\\u5408\\u610f" in out.stdout and "\\xff" in out.stdout and implement_run.CALLERS_HEAD in out.stdout
            assert file.read_text(encoding="utf-8").startswith("Status: active ascii-owner #")
            assert implement_run.CALLERS_HEAD in file.read_text(encoding="utf-8")
    finally:
        file.write_text(original, encoding="utf-8")
        contracts.write_text(original_contracts, encoding="utf-8")
        app.write_bytes(original_app)


def test_closed_stdout(repo, env, folder):
    file = folder / "01-frame.md"
    original = file.read_text()
    file.write_text(original + "large output\n" * 10000)
    try:
        with subprocess.Popen([sys.executable, SCRIPT, "take", str(folder), "01-frame", "pipe"],
                              cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            with subprocess.Popen(["head", "-c", "10"], stdin=child.stdout, stdout=subprocess.PIPE) as consumer:
                child.stdout.close()
                output, _ = consumer.communicate()
                assert consumer.returncode == 0 and len(output) == 10
            errors = child.stderr.read()
            assert child.wait() == 0 and not errors, errors
    finally:
        file.write_text(original)


def test_block_ascii_locale(tmp):
    """start reads .git/info/exclude, domains.md, profile.md, the block text (`…` in web-ui-basics), and catalog skill text as
    UTF-8 under an ASCII locale."""
    assert_ascii_locale()
    skill = Path(tmp, "ascii-skill.md")
    skill.write_text("# skill\nRULE § 9\n", encoding="utf-8")
    out, folder, _, run = blocks_start(tmp, "ascii", f"# é note\n- build: block:web-ui-basics, {skill}\n", env=ASCII_ENV)
    assert out.returncode == 0, out.stderr
    assert "…" in step_text(folder, "05-build") and "RULE § 9" in step_text(folder, "05-build")
    Path(tmp, "ascii", "home", ".flow", "app", "profile.md").write_text("# é note\nDomains: any\nRepo rules: none\n",
                                                                       encoding="utf-8")
    # A non-ASCII line in .git/info/exclude and no /tmp/ line yet: start reads it as UTF-8 and appends /tmp/.
    exclude = Path(tmp, "ascii", "app", ".git", "info", "exclude")
    kept = [line for line in exclude.read_text(encoding="utf-8").splitlines() if line != "/tmp/"]
    exclude.write_text("\n".join(["# café note", *kept]) + "\n", encoding="utf-8")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0 and "RULE § 9" in step_text(folder, "05-build"), out.stderr
    assert "could not add /tmp/" not in out.stderr, out.stderr
    lines = exclude.read_text(encoding="utf-8").splitlines()
    assert lines[0] == "# café note" and lines[-1] == "/tmp/", lines
    # An exclude file that is not UTF-8 gets a one-line note, and start goes on.
    exclude.write_bytes(b"\xff\n")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0 and "could not add /tmp/" in out.stderr and "Traceback" not in out.stderr, out.stderr


def test_review_read_encoding(tmp):
    """take 06-review reads 06-review.md, and take 07-close also reads the verdict and target.json, all
    UTF-8 text, under an ASCII locale."""
    assert_ascii_locale()
    out, folder, repo, run = blocks_start(tmp, "review-ascii", "- review: block:web-ui-basics\n", env=ASCII_ENV)
    assert out.returncode == 0 and "…" in step_text(folder, "06-review"), out.stderr
    subprocess.run(["git", "commit", "-q", "--allow-empty", "-m", "one"], env=git_env(),
                   cwd=repo, check=True)

    def mark(step, extra=""):
        f = folder / f"{step}.md"
        f.write_text("Status: done x\n" + f.read_text(encoding="utf-8").split("\n", 1)[1] + extra, encoding="utf-8")

    for step in ("01-frame", "02-design", "03-contracts", "04-impact", "05-build"):
        mark(step)
    out = run("take", folder, "06-review", "a")
    assert out.returncode == 0, out.stderr
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=repo, env=git_env(),
                          capture_output=True, text=True, check=True).stdout.strip()
    review = repo / "tmp" / "review-check" / "range-1"
    review.mkdir(parents=True)
    (review / "03-verdict.md").write_text("Status: done x\nUses:\nVerdict: APPROVE (1 of 1 units; 0 fix)\n- note: …\n", encoding="utf-8")
    (review / "target.json").write_text(json.dumps({"head": head, "note": "…"}, ensure_ascii=False), encoding="utf-8")
    mark("06-review", "Run: tmp/review-check/range-1\n")
    # A verdict file that is not UTF-8 stops take with one line, as the other implement files do.
    good = (review / "03-verdict.md").read_bytes()
    (review / "03-verdict.md").write_bytes(b"Status: x\n\nVerdict: APPROVE \xff\xfe\n")
    out = run("take", folder, "07-close", "a")
    assert out.returncode == 1 and "03-verdict.md is not UTF-8" in out.stderr and len(out.stderr.splitlines()) == 1, out.stderr
    (review / "03-verdict.md").write_bytes(good)
    # So does a target.json with bad bytes, bad JSON, a value that is not an object, or a head that is not a string.
    target = (review / "target.json").read_bytes()
    for data, reason in ((b'{"head": "abc\xff"}', "target.json is not UTF-8"), (b'{"head": ', "target.json is not valid JSON"),
                         (b"[" * 10**6 + b"]" * 10**6, "target.json is not valid JSON"),  # RecursionError
                         (b"[1]", "target.json is not a JSON object"), (b'{"head": 3}', "covers no head")):
        (review / "target.json").write_bytes(data)
        out = run("take", folder, "07-close", "a")
        assert out.returncode == 1 and reason in out.stderr and len(out.stderr.splitlines()) == 1, (data, out.stderr)
    (review / "target.json").write_bytes(target)
    out = run("take", folder, "07-close", "a")
    assert out.returncode == 0, out.stderr
