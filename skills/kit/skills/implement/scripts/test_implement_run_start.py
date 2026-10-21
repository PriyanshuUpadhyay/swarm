# Implement test start cases and helpers.
from test_implement_run_support import *


def test_installed_kit_paths(tmp):
    out, folder, repo, run = blocks_start(tmp, "stable-paths", "", None)
    assert out.returncode == 0, out.stderr
    home = repo.parent / "home"
    (home / ".swarm").mkdir()
    (home / ".swarm/kit").symlink_to(SCRIPT.parents[3], target_is_directory=True)
    (home / ".flow/domains.md").write_text(
        "## any\n- build: ~/.swarm/kit/blocks/prove-it-works/SKILL.md, block:prove-it-works\n", encoding="utf-8")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0, out.stderr
    text = step_text(folder, "05-build")
    assert "Take with: python3 ~/.swarm/kit/skills/implement/scripts/implement_run.py take " in text, text
    assert "Skills read: ~/.swarm/kit/blocks/prove-it-works/SKILL.md" in text, text
    assert "under ~/.swarm/kit (the kit root)" in text, text
    assert str(SCRIPT.parents[3]) not in text and str(SCRIPT) not in out.stdout, out
    # A source checkout without an installed link must keep its own paths.
    local = repo.parent / "source-home"
    local.mkdir()
    with patch.object(implement_run.step_run.Path, "home", return_value=local):
        assert implement_run.step_run.kit_path(SCRIPT) == str(SCRIPT)


def test_start_plan_needs_approval(tmp):
    """`start --plan <run>` refuses a plan that the plan skill's stop rule does not pass, and names it in 01-frame."""
    import hashlib
    plan = Path(tmp, "plan-run")
    plan.mkdir()
    table = implement_table().replace("| checked-units, prove-it-works, task-file |", "| checked-units, ui-motion |")
    (plan / "plan.md").write_text(f"# Plan: x\n\nPreset: implement\n\n{table}", encoding="utf-8")
    (plan / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
    out, folder, repo, run = blocks_start(tmp, "with-plan", "", None, "--plan", plan)
    assert out.returncode == 1 and "no round in" in out.stderr and not Path(repo, "tmp", "flow").exists(), out
    files = {f"../{n}": hashlib.sha256((plan / n).read_bytes()).hexdigest() for n in ("plan.html", "plan.md")}
    (plan / "plan-review").mkdir()
    (plan / "plan-review" / "1.json").write_text(json.dumps({"files": files, "result": {"decision": "approved"}}))
    out = run("start", "--plan", plan)
    assert out.returncode == 0, out.stderr
    assert f"Plan: {plan.resolve()}\n" in step_text(Path(out.stdout.splitlines()[0]), "01-frame")
    assert "Plan blocks: checked-units, ui-motion\n" in step_text(Path(out.stdout.splitlines()[0]), "05-build"), \
        "the step file carries the blocks that the approved plan gives the step"
    # A chat preset passes the stop rule with no round, but it is not an implement plan.
    explain = SCRIPT.parents[1].parent / "explain" / "SKILL.md"
    chat = Path(tmp, "chat-run")
    chat.mkdir()
    table = explain.read_text(encoding="utf-8").split("## Preset\n", 1)[1].split("\n\n", 2)[1]
    (chat / "plan.md").write_text(f"# Plan: x\n\nPreset: explain\n\n{table}", encoding="utf-8")
    (chat / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
    shutil.rmtree(Path(out.stdout.splitlines()[0]))
    out = run("start", "--plan", chat)
    assert out.returncode == 1 and "needs `Preset: implement`" in out.stderr, out


def implement_table():
    """The implement preset's step table, as a plan copies it."""
    return (SCRIPT.parents[1] / "SKILL.md").read_text(encoding="utf-8").split("## Preset\n", 1)[1].split("\n\n", 3)[2] + "\n"


def approve(plan):
    import hashlib
    files = {f"../{n}": hashlib.sha256((plan / n).read_bytes()).hexdigest() for n in ("plan.html", "plan.md")}
    (plan / "plan-review").mkdir(exist_ok=True)
    (plan / "plan-review" / "1.json").write_text(json.dumps({"files": files, "result": {"decision": "approved"}}))


def test_start_plan_must_match_the_graph(tmp):
    """An approved plan whose steps or Needs differ from implement's graph is refused, so no approved step is dropped."""
    full = implement_table()
    probe = "| `04-probe.md` | frame | CI logs | local-research | find the cause |\n"
    cases = {
        "added": (full.replace("| `05-build.md` |", probe + "| `05-build.md` |", 1), ["added 04-probe"]),
        "missing": ("\n".join(l for l in full.splitlines() if "`02-design.md`" not in l) + "\n",
                    ["missing 02-design", "skip 02-design, 03-contracts or 04-impact after 01-frame"]),
        "needs": (full.replace("| `04-impact.md` | contracts |", "| `04-impact.md` | frame |", 1),
                  ["04-impact needs 03-contracts in implement, 01-frame in plan"]),
    }
    run = None
    for name, (table, words) in cases.items():
        plan = Path(tmp, f"plan-{name}")
        plan.mkdir()
        (plan / "plan.md").write_text(f"# Plan: x\n\nPreset: implement\n\n{table}", encoding="utf-8")
        (plan / "plan.html").write_text("<p>chart</p>", encoding="utf-8")
        approve(plan)
        if run is None:
            out, folder, repo, run = blocks_start(tmp, "graph-match", "", None, "--plan", plan)
        else:
            out = run("start", "--plan", plan)
        assert out.returncode == 1, (name, out)
        assert "differ from implement's graph" in out.stderr and all(w in out.stderr for w in words), (name, out.stderr)
        assert not Path(repo, "tmp", "flow").exists(), f"{name}: a refused plan makes no run folder"


def test_auto_mode_needs_the_owners_words(tmp):
    """`take 05-build` refuses `Mode: auto` unless 01-frame quotes the owner, so the model cannot pick auto alone."""
    out, folder, repo, run = blocks_start(tmp, "auto-guard", "", None)
    assert out.returncode == 0, out.stderr
    for step in ("02-design", "03-contracts", "04-impact"):
        assert run("skip", folder, step, "fast lane: test").returncode == 0
    frame = folder / "01-frame.md"
    body = frame.read_text(encoding="utf-8").replace("Status: open", "Status: done x", 1)
    variants = ("Mode: auto\n", 'Mode: auto, owner said: ""\n', "Mode: Auto\n", "Mode:  auto\n", "Mode: full auto\n",
                "Mode: deliver\n", "Mode: pair\nMode: auto\n", "mode: auto\n", "- Mode: auto\n", "Mode: pair \n")
    for mode, ok in [(v, False) for v in variants] + [('Mode: auto, owner said: "run it to the end"\n', True)]:
        frame.write_text(body.replace("Domains:", mode + "Domains:", 1), encoding="utf-8")
        out = run("take", folder, "05-build", "a", "--force")
        assert (out.returncode == 0) == ok, (mode, out.stderr)
        if not ok:
            assert 'one line `Mode: pair` or `Mode: auto, owner said: "<the owner\'s words>"`' in out.stderr, (mode, out.stderr)
    frame.write_text(body.replace("Domains:", "Mode: pair\nDomains:", 1), encoding="utf-8")
    assert run("take", folder, "05-build", "a", "--force").returncode == 0, "pair needs no quote"


def test_start_domains(tmp):
    """A run selects profile domains once, always keeps any, and rejects unknown domains."""
    for name, selected in (("selected", True), ("all", False)):
        _, _, repo, run = blocks_start(tmp, "domains-" + name, "", None, "--domains", "rust")
        home = repo.parent / "home/.flow"
        (home / "app/profile.md").write_text("Domains: rust, zig\nRepo rules: none\n", encoding="utf-8")
        catalog = []
        for domain in ("any", "rust", "zig"):
            rule = repo / f"{domain}.md"
            rule.write_text(f"RULE {domain}\n", encoding="utf-8")
            catalog.append(f"## {domain}\n- build: {rule}\n")
        (home / "domains.md").write_text("".join(catalog), encoding="utf-8")
        unknown = run("start", "--domains", "swift")
        assert unknown.returncode == 1 and "swift" in unknown.stderr, unknown
        assert "any, rust, zig" in unknown.stderr, unknown.stderr
        out = run("start", *( ["--domains", "rust", "--profile", "default"] if selected else []))
        assert out.returncode == 0, out.stderr
        folder = Path(out.stdout.splitlines()[0])
        build = step_text(folder, "05-build")
        assert "RULE any" in build and "RULE rust" in build, build
        assert ("RULE zig" in build) == (not selected), build
        choice = "Domains: any, rust" + ("" if selected else ", zig")
        assert choice + "\n" in step_text(folder, "01-frame") and choice in out.stdout
        before = {p.name: p.read_bytes() for p in folder.iterdir()}
        again = run("start", "--domains", "zig")
        assert again.returncode == 0 and choice + "\n" in again.stdout, again
        assert {p.name: p.read_bytes() for p in folder.iterdir()} == before


def test_start_block_ledger(tmp):
    """Start shows every non-base block, actual loading steps, and off reasons, once in frame."""
    import tomllib
    catalog = "- design: block:tech-diagram\n- build: block:html-diagram, block:prove-it-works\n"
    config = PARAMS + '[off]\nwhen-to-diagram = "no diagram choice\\nneeded | here"\nstory = "no story"\n'
    out, folder, repo, run = blocks_start(tmp, "ledger", catalog, config)
    assert out.returncode == 0, out.stderr
    assert "### Chosen\n" in out.stdout and "### Rejected\n" in out.stdout, out.stdout
    tables = out.stdout[out.stdout.index("### Chosen\n"):].strip()
    frame = step_text(folder, "01-frame")
    assert tables in frame and frame.count("## Blocks considered\n") == 1, frame
    assert all("## Blocks considered\n" not in step_text(folder, step) for step in implement_run.graph() if step != "01-frame")
    kit = tomllib.loads((implement_run.BLOCKS / "blocks.toml").read_text(encoding="utf-8"))
    expected = {p.parent.name for p in implement_run.BLOCKS.glob("*/SKILL.md")} - set(kit["base"])
    rows = [line.split("|")[1].strip().removeprefix("↳ ") for line in tables.splitlines() if line.startswith("| ")]
    names = [block for name in rows if name != "Block" and not name.endswith("(alias)") for block in name.split(", ")]
    assert len(names) == len(set(names)) and set(names) == expected, names
    chosen, rejected = tables.split("### Rejected\n")
    assert {name for name in expected if f"| {name} |" in chosen or f"| ↳ {name} |" in chosen} == {"html-diagram", "prove-it-works"}
    assert "| tech-diagram (alias) | |\n| ↳ html-diagram | Loaded by design, build |" in chosen, chosen
    assert "| prove-it-works | Loaded by build |" in chosen, chosen
    assert "| ↳ when-to-diagram | [off] no diagram choice needed &#124; here |" in rejected, rejected
    assert "| story | [off] no story |" in rejected, rejected
    assert "not named by a domain line" in rejected, rejected
    assert rejected.count("not named by a domain line") == 1, rejected
    assert not has_control(out.stdout), out.stdout
    before = {p.name: p.read_bytes() for p in folder.iterdir()}
    (repo.parent / "home/.flow/domains.md").write_text("## any\n- build: block:nope\n", encoding="utf-8")
    (repo / ".blocks.toml").write_text("not valid TOML", encoding="utf-8")
    again = run("start")
    assert again.returncode == 0 and tables in again.stdout, again
    assert {p.name: p.read_bytes() for p in folder.iterdir()} == before


def test_start_ledger_without_chosen_blocks(tmp):
    out, folder, _, _ = blocks_start(tmp, "ledger-empty", "", None)
    assert out.returncode == 0, out.stderr
    chosen, rejected = out.stdout.split("### Chosen\n")[1].split("### Rejected\n")
    assert chosen.strip() == "| Block | Why |\n|---|---|", chosen
    assert "html-diagram" in rejected and rejected.count("not named by a domain line") == 1, rejected
    assert "base-rules" not in out.stdout and "base-rules" not in step_text(folder, "01-frame")


def test_git_env_ignores_caller_git_dir(tmp):
    """A run from a git hook inherits GIT_DIR, GIT_WORK_TREE, and GIT_INDEX_FILE; the tests' git calls and implement
    runs still use their own repo, so the caller's repo gets no commit and no tmp/flow folder."""
    decoy = Path(tmp, "decoy")
    subprocess.run(["git", "init", "-q", "-b", "main", str(decoy)], env=git_env(), check=True)
    hook = {"GIT_DIR": str(decoy / ".git"), "GIT_WORK_TREE": str(decoy), "GIT_INDEX_FILE": str(decoy / ".git" / "index")}
    with patch.dict(os.environ, hook):
        out, folder, repo, _ = blocks_start(tmp, "hooked", PROVE_BUILD)
        subprocess.run(["git", "commit", "-q", "--allow-empty", "-m", "one"], cwd=repo, env=git_env(), check=True)
    assert out.returncode == 0 and folder.resolve().is_relative_to(repo.resolve()), out
    log = subprocess.run(["git", "log", "--oneline"], cwd=decoy, env=git_env(), capture_output=True, text=True)
    assert log.stdout == "" and not (decoy / "tmp").exists(), log
