# Implement test configuration cases and helpers.
from test_implement_run_support import *


def test_start_without_tomllib(tmp):
    """Without tomllib, status still runs; a fresh start needs it to list the shipped catalog."""
    out, folder, repo, _ = blocks_start(tmp, "no-tomllib", PROVE_BUILD)
    assert out.returncode == 0, out.stderr
    code = "import runpy, sys; sys.modules['tomllib'] = None; sys.argv = sys.argv[1:]; runpy.run_path(sys.argv[0], run_name='__main__')"
    env = {**git_env(), "HOME": str(Path(tmp, "no-tomllib", "home"))}
    run = lambda *a: subprocess.run([sys.executable, "-I", "-c", code, str(SCRIPT), *a], cwd=repo, env=env,
                                    capture_output=True, text=True)
    out = run("status")
    assert out.returncode == 0 and "01-frame" in out.stdout, out
    shutil.rmtree(folder)  # so start must read the block catalog again
    out = run("start")
    assert out.returncode == 1 and out.stderr == f"{implement_run.BLOCKS / 'blocks.toml'}: reading TOML needs Python 3.11 or later (tomllib); use a newer python3, for example from Homebrew\n", out
    (repo.parent / "home/.flow/domains.md").write_text("## any\n", encoding="utf-8")
    out = run("start")
    assert out.returncode == 1 and out.stderr == f"{implement_run.BLOCKS / 'blocks.toml'}: reading TOML needs Python 3.11 or later (tomllib); use a newer python3, for example from Homebrew\n", out


def test_start_kit_without_blocks(tmp):
    """A kit copy without blocks still starts without tomllib, with empty choice tables."""
    kit = Path(tmp, "kit-no-blocks")
    for part in ("skills/implement/scripts/implement_run.py", "skills/implement/scripts/implement_context.py",
                 "skills/implement/SKILL.md", "references/step_run.py", "references/step_checks.py",
                 "references/step_scope.py", "references/step_brief.py",
                 "references/block_ledger.py", "references/block_resolver.py"):
        (kit / part).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(SCRIPT.parents[3] / part, kit / part)
    _, before, _, _ = blocks_start(tmp, "no-block-a", "", None)
    _, made, repo, _ = blocks_start(tmp, "no-block-b", "", None)
    shutil.rmtree(made)
    code = "import runpy, sys; sys.modules['tomllib'] = None; sys.argv = sys.argv[1:]; runpy.run_path(sys.argv[0], run_name='__main__')"
    out = subprocess.run([sys.executable, "-I", "-c", code, str(kit / "skills/implement/scripts/implement_run.py"), "start"], cwd=repo,
                         env={**git_env(), "HOME": str(Path(tmp, "no-block-b", "home"))}, capture_output=True, text=True)
    assert out.returncode == 0 and out.stderr == "", out
    assert "### Chosen\n\n| Block | Why |\n|---|---|\n\n### Rejected\n\n| Block | Why |\n|---|---|" in out.stdout
    mask = lambda text, *paths: [text := text.replace(str(p), "<p>") for p in paths][-1]
    for file in sorted(before.glob("0*.md")):
        a = mask(file.read_text(encoding="utf-8"), before.parents[2], SCRIPT.parents[3])
        b = mask((made / file.name).read_text(encoding="utf-8"), kit.resolve(), made.parents[2], kit)
        a, b = [re.sub(r"## Blocks considered\n.*?(?=## Rules for this step)", "", text, flags=re.S) for text in (a, b)]
        assert a == b, (file.name, a, b)
        assert "profile " not in next(l for l in b.splitlines() if l.startswith("Skills read:")), b


def test_toml_no_block_adds_no_base(tmp):
    """A repo with a .blocks.toml and a catalog that names no block gets no base block, so a live repo that adds
    the file before it names a block writes what it wrote before."""
    out, folder, _, _ = blocks_start(tmp, "toml-no-block", "", PARAMS)
    assert out.returncode == 0, out.stderr
    for file in sorted(folder.glob("0*.md")):
        text = file.read_text(encoding="utf-8")
        assert "Skills read: none" in text.splitlines() and "base-rules" not in text, (file.name, text)


def test_no_toml_runs_blocks_without_base(tmp):
    """A repo with no .blocks.toml still runs a block that needs no param, such as one an `any` catalog line
    names; the base block, which needs source_of_truth, is off with its reason."""
    out, folder, _, _ = blocks_start(tmp, "no-toml", "- close: block:done-when-evidence\n", None)
    assert out.returncode == 0, out.stderr
    close = step_text(folder, "07-close")
    assert "===== block:done-when-evidence =====" in close and "===== block:base-rules" not in close, close
    assert "Off: base-rules: the repo has no .blocks.toml, which sets source_of_truth" in close, close
    # A named block that needs a param still stops start, and the message names the file to make.
    out, _, repo, _ = blocks_start(tmp, "no-toml-param", "- build: block:web-ui-basics\n", None)
    assert out.returncode == 1 and "set it under [params] in" in out.stderr and ".blocks.toml" in out.stderr, out.stderr
    assert not (repo / "tmp" / "flow").exists(), "a failed start must not leave a folder"


def test_param_quoted_in_script_checks(tmp):
    """A [params] value fills a Check (script) command as one shell word, so a space or an option inside it
    stays one path; rules text gets the value as written."""
    toml = PARAMS.replace('site = "site"', 'site = "my site --files"')
    out, folder, repo, _ = blocks_start(tmp, "param-quote", "- build: block:web-ui-basics\n- design: block:design-tokens\n", toml)
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    todo = next(l for l in build.splitlines() if "'prefers-reduced-motion'" in l)
    command = re.search(r"`([^`]+)`", todo)[1]
    assert command.endswith(" 'my site --files'"), todo
    assert "`my site --files/DESIGN.md`" in step_text(folder, "02-design").split(implement_run.TODO_HEAD)[0], "rules stay plain"
    (repo / "my site --files").mkdir()
    (repo / "my site --files" / "a.css").write_text("@media (prefers-reduced-motion: reduce) {}\n", encoding="utf-8")
    ran = subprocess.run(command, shell=True, cwd=repo, capture_output=True, text=True)
    assert ran.returncode == 0 and ran.stdout.startswith("my site --files/a.css:1:"), ran


def test_param_outside_span_in_script_check(tmp):
    """In a Check (script) line, a {{name}} outside a backtick span is filled as plain text, as in Rules; only
    inside a span is it quoted."""
    Path(tmp, "param-prose", "app", "docs").mkdir(parents=True)
    Path(tmp, "param-prose", "app", "docs", "x.md").write_text(
        "# x\n\n## Rules\n- r\n\n## Check (script)\n- Look at {{site}} outside backticks and run `ls {{site}}`: prose.\n\n## Check (judge)\n",
        encoding="utf-8")
    toml = PARAMS.replace('site = "site"', 'site = "my site"') + '[add]\nbuild = ["docs/x.md"]\n'
    out, folder, _, _ = blocks_start(tmp, "param-prose", PROVE_BUILD, toml)
    assert out.returncode == 0, out.stderr
    todo = next(l for l in step_text(folder, "05-build").splitlines() if l.startswith("- [ ] block x script:"))
    assert todo == "- [ ] block x script: Look at my site outside backticks and run `ls 'my site'`: prose.: ", todo


def test_add_path_errors(tmp):
    """An [add] path that is a symlink loop or a too-long name gives the one-line "is not a file" reason, also
    on Python 3.11 and 3.12, where resolve and is_file raise for them; those run when uv can find them."""
    found = [subprocess.run(["uv", "python", "find", v], capture_output=True, text=True).stdout.strip()
             for v in ("3.11", "3.12")] if shutil.which("uv") else []
    pythons = [sys.executable] + [p for p in found if p and Path(p).resolve() != Path(sys.executable).resolve()]
    for i, python in enumerate(pythons):
        for case, path in (("loop", "loop.md"), ("long", "a" * 300 + ".md")):
            name = f"add-{case}-{i}"
            Path(tmp, name, "app").mkdir(parents=True)
            if case == "loop":
                Path(tmp, name, "app", path).symlink_to(path)
            out, _, _, _ = blocks_start(tmp, name, PROVE_BUILD, PARAMS + f'[add]\nbuild = ["{path}"]\n', python=python)
            assert out.returncode == 1 and out.stderr.startswith(f"[add] {path} in ") and out.stderr.endswith(
                " is not a file\n") and len(out.stderr.splitlines()) == 1, (python, case, out.stderr[-300:])
