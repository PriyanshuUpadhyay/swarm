# Implement test blocks cases and helpers.
from test_implement_run_support import *


def test_block_in_step(tmp):
    """A catalog step line naming block:<id> writes that block into the step file, and the
    Skills read line names the profile and the block with its content hash."""
    out, folder, repo, run = blocks_start(tmp, "d1", PROVE_BUILD)
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert "===== block:prove-it-works =====" in build and "Rank each claim on the ladder" in build, build
    skills = next(line for line in build.splitlines() if line.startswith("Skills read:"))
    assert "profile default" in skills, skills
    digest = hashlib.sha256((implement_run.BLOCKS / "prove-it-works" / "SKILL.md").read_bytes()).hexdigest()[:8]
    assert f"block:prove-it-works@{digest}" in [item.strip() for item in skills.removeprefix("Skills read:").split(",")], skills
    assert "Rank each claim on the ladder" not in step_text(folder, "02-design")
    assert f"are under {implement_run.BLOCKS.parent} (the kit root)." in build
    # The base and proof blocks point to evidence owners; use a path-free block to check the other case.
    no_path, no_path_folder, _, _ = blocks_start(tmp, "no-kit-path", "- build: block:voice-reply\n", None)
    assert no_path.returncode == 0, no_path.stderr
    assert "(the kit root)" not in step_text(no_path_folder, "05-build"), "no kit path needs no kit-root line"
    # A block that names a kit path gets the kit root, because the step file is read outside the kit.
    kit_out, kit_folder, _, _ = blocks_start(tmp, "kit-root", "- build: block:task-file\n")
    assert kit_out.returncode == 0, kit_out.stderr
    assert f"are under {implement_run.BLOCKS.parent} (the kit root)." in step_text(kit_folder, "05-build")
    # A second start on the folder writes nothing, even when the catalog now fails.
    (repo.parent / "home" / ".flow" / "domains.md").write_text("## any\n- build: block:nope\n", encoding="utf-8")
    again = run("start")
    assert again.returncode == 0 and step_text(folder, "05-build") == build, again.stderr


def test_block_alias_and_base(tmp):
    """An alias expands to its members, duplicates drop, and base joins every step once."""
    out, folder, _, _ = blocks_start(tmp, "d2", "- design: block:tech-diagram, block:html-diagram\n" + PROVE_BUILD)
    assert out.returncode == 0, out.stderr
    design = step_text(folder, "02-design")
    for block in ("base-rules", "when-to-diagram", "html-diagram"):
        assert design.count(f"===== block:{block} =====") == 1, (block, design)
    # html-diagram comes twice, through the alias and alone; a merged group prints one header either way.
    skills = next(line for line in design.splitlines() if line.startswith("Skills read: "))
    assert skills.count("block:html-diagram@") == 1, skills
    assert len(set(rule_lines(design))) == len(rule_lines(design)), rule_lines(design)
    for step in ("01-frame", "02-design", "03-contracts", "04-impact", "05-build", "06-review", "07-close"):
        assert step_text(folder, step).count("===== block:base-rules =====") == 1, step


def test_block_hash_bytes(tmp):
    """The Skills read hash is the sha256 of the block file's bytes, also for a file with CRLF line ends."""
    Path(tmp, "crlf", "app", "docs").mkdir(parents=True)
    data = b"# crlf\r\n\r\n## Rules\r\n- CRLF RULE 3\r\n## Check (script)\r\n- none: x\r\n## Check (judge)\r\n- Is it CRLF?\r\n"
    Path(tmp, "crlf", "app", "docs", "crlf.md").write_bytes(data)
    out, folder, _, _ = blocks_start(tmp, "crlf", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/crlf.md"]\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    skills = next(line for line in build.splitlines() if line.startswith("Skills read:"))
    assert f"block:crlf@{hashlib.sha256(data).hexdigest()[:8]}" in skills, skills
    assert "- CRLF RULE 3\n" in build and "\r" not in build, build


def test_block_profile(tmp):
    """The merged profile writes each alias as one block and folds base in, with the same rule and
    check lines as the split form, and drops the Example sections."""
    catalog = "- design: block:tech-diagram, block:style-frames, block:design-tokens\n"
    split, split_folder, _, _ = blocks_start(tmp, "d3-split", catalog)
    merged, merged_folder, _, _ = blocks_start(tmp, "d3-merged", catalog, PARAMS, "--profile", "merged")
    assert split.returncode == 0 and merged.returncode == 0, (split.stderr, merged.stderr)
    a, b = step_text(split_folder, "02-design"), step_text(merged_folder, "02-design")
    assert a.count("===== block:") == 8 and b.count("===== block:") == 5, b
    assert "===== block:tech-diagram =====" in b and "===== block:style-frames =====" in b, b
    assert "\n### Example\n" in a and "Example\n" not in b
    assert "profile merged" in b.splitlines()[3], b.splitlines()[3]
    assert rule_lines(a) == rule_lines(b), set(rule_lines(a)) ^ set(rule_lines(b))
    assert rule_lines(step_text(split_folder, "06-review")) == rule_lines(step_text(merged_folder, "06-review"))
    assert split.stdout.split("### Chosen\n")[1] == merged.stdout.split("### Chosen\n")[1], "packaging keeps the same choices"


def test_profile_on_second_start(tmp):
    """A second start checks the profile name too, and says a known non-default profile was not applied."""
    out, folder, _, run = blocks_start(tmp, "d3-again", PROVE_BUILD)
    assert out.returncode == 0, out.stderr
    before = {p.name: p.read_bytes() for p in folder.iterdir()}
    again = run("start", "--profile", "nosuch")
    assert again.returncode == 1 and again.stderr.startswith("unknown profile nosuch;"), again
    again = run("start", "--profile", "merged")
    assert again.returncode == 0 and again.stdout.splitlines()[0] == str(folder), again
    assert f"note: {folder} exists, so --profile merged was not applied" in again.stderr, again.stderr
    for args in (("--profile", "default"), ()):
        again = run("start", *args)
        assert again.returncode == 0 and "was not applied" not in again.stderr, (args, again.stderr)
    assert {p.name: p.read_bytes() for p in folder.iterdir()} == before, "a second start must write nothing"


def test_block_example_params(tmp):
    """A {{param}} only in a block's Example would make every run set it for nothing, so no kit block has
    one; a repo with only source_of_truth starts a close step that names done-when-evidence."""
    for path in sorted(implement_run.BLOCKS.glob("*/SKILL.md")):
        # The loader drops the frontmatter, so a {{param}} there is never filled and must not count.
        parts = block_resolver.sections(path.read_text(encoding="utf-8").partition("\n---\n")[2])
        rest = {p for head, lines in parts if head != "Example" for p in block_resolver.PARAM.findall("\n".join(lines))}
        example = {p for head, lines in parts if head == "Example" for p in block_resolver.PARAM.findall("\n".join(lines))}
        assert example <= rest, (path.parent.name, example - rest)
    root = Path(tmp, "example-params")
    root.mkdir()
    (root / ".blocks.toml").write_text('[params]\nsource_of_truth = "README.md"\n', encoding="utf-8")
    for profile in ("default", "merged"):
        implement_run.resolve_blocks(root, {"close": ["done-when-evidence"]}, profile)


def test_block_params(tmp):
    """A {{name}} takes its value from the repo's .blocks.toml; a missing one stops start and names it."""
    out, folder, _, _ = blocks_start(tmp, "d4", "- review: block:diagram-check\n")
    assert out.returncode == 0, out.stderr
    review = step_text(folder, "06-review")
    assert "The source of truth is docs/spec.md." in review and "{{" not in review, review
    out, folder, repo, _ = blocks_start(tmp, "d4-missing", "- review: block:diagram-check\n", "")
    assert out.returncode == 1 and "{{source_of_truth}}" in out.stderr and "Traceback" not in out.stderr, out.stderr
    assert not (repo / "tmp" / "flow").exists(), "a failed start must not leave a folder"


def test_block_controls(tmp):
    """Control bytes from the repo's [params] values and [add] block files never reach a step file, and a
    newline in a param value cannot add a todo."""
    Path(tmp, "controls-text", "app", "docs").mkdir(parents=True)
    Path(tmp, "controls-text", "app", "docs", "x.md").write_text(
        "# x\nConcept: x\n\n## Rules\n- RULE \x1b[2J 7\n\n## Check (script)\n- `true` runs: ok\n\n## Check (judge)\n", encoding="utf-8")
    params = PARAMS.replace('site = "site"', 'site = "s\\u001b[2Jx\\n- [ ] injected"')
    out, folder, _, _ = blocks_start(tmp, "controls-text", "- build: block:web-ui-basics\n", params + '[add]\nbuild = ["docs/x.md"]\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert not has_control(build), "no control bytes in a step file"
    assert "RULE [2J 7" in build and "s[2Jx - [ ] injected" in build, build
    # A control byte inside a placeholder is dropped before the param check, so a missing name gets one line.
    Path(tmp, "controls-param", "app", "docs").mkdir(parents=True)
    Path(tmp, "controls-param", "app", "docs", "y.md").write_text("# y\n\n## Rules\n- see {{no\x1bpe}}\n", encoding="utf-8")
    out, _, repo, _ = blocks_start(tmp, "controls-param", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/y.md"]\n')
    assert out.returncode == 1 and "block y needs {{nope}}" in out.stderr and "Traceback" not in out.stderr, out.stderr
    assert not (repo / "tmp" / "flow").exists()
    # Count the block's script checks with a plain read, not with the code that writes the todos.
    section = (implement_run.BLOCKS / "web-ui-basics" / "SKILL.md").read_text(encoding="utf-8").split("\n## Check (script)\n", 1)[1].split("\n## ", 1)[0]
    expected = [line for line in section.splitlines() if line.startswith("- ") and not line.startswith("- none:")]
    assert len(expected) >= 2 and build.count("block web-ui-basics script:") == len(expected), build


def test_block_headings(tmp):
    """A block's or a catalog skill's `## ` sections are written as `### ` under its marker, so text that
    holds a `## Todo (` list and a `## Result` section cannot stand in for the step's own todo list; the
    skill's control bytes are dropped too."""
    Path(tmp, "fake-todo", "app", "docs").mkdir(parents=True)
    Path(tmp, "fake-todo", "app", "docs", "fake.md").write_text(
        "# fake\n\n## Todo (fake)\n- [x] nothing: ok\n\n## Result\nall good\n\n## Check (script)\n- none: x\n\n## Check (judge)\n- Is it fake?\n",
        encoding="utf-8")
    skill = Path(tmp, "fake-skill.md")
    skill.write_text("# skill\n\n## Todo (x)\n- [x] a: ok\n\n## Result\nfine \x1b[2J\n", encoding="utf-8")
    for name, catalog, repo_toml, kept in (
            ("fake-todo", PROVE_BUILD, PARAMS + '[add]\ndesign = ["docs/fake.md"]\n', ["### Todo (fake)"]),
            ("fake-skill", f"- design: {skill}\n", PARAMS, ["# skill\n", "### Todo (x)"])):
        out, folder, _, run = blocks_start(tmp, name, catalog, repo_toml)
        assert out.returncode == 0, out.stderr
        design = step_text(folder, "02-design")
        assert not has_control(design), design
        rules = design.split(implement_run.RULES_HEAD, 1)[1].split(implement_run.TODO_HEAD, 1)[0]
        assert all(k in rules for k in kept) and not any(line.startswith("## ") for line in rules.splitlines()), rules
        done = run("done", folder, "02-design")
        assert done.returncode == 1 and "- [ ] " in done.stderr and "no todo list" not in done.stderr, (name, done.stderr)


def test_skill_fenced_headings(tmp):
    """A catalog skill's `## ` line inside a ``` or ~~~ fence is quoted format and stays as is; one outside a
    fence becomes `### `, and so does a fenced `## Todo (` line, which could still pass for a todo head."""
    skill = Path(tmp, "fenced-skill.md")
    skill.write_text("# s\n## Report\n```text\n## src/Button.tsx\n## Todo (x)\n```\n~~~\n## also kept\n~~~\n## After\n",
                     encoding="utf-8")
    lines = implement_run.skills_text([str(skill)]).splitlines()[1:]
    assert lines == ["# s", "### Report", "```text", "## src/Button.tsx", "### Todo (x)", "```", "~~~", "## also kept",
                     "~~~", "### After"], lines
    # As in CommonMark, only a bare fence line closes a fence; a line such as ```js inside one is content.
    assert implement_run.demote("```text\n## kept\n```js\nx\n```\n## after") == "```text\n## kept\n```js\nx\n```\n### after"
