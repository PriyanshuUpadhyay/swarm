# Implement test block metadata cases and helpers.
from test_implement_run_support import *


def test_block_frontmatter(tmp):
    """A block's frontmatter description is the step's Concept line, no frontmatter line reaches the step,
    and a name that is not the block id stops start."""
    kit = Path(tmp, "front")
    (kit / "a").mkdir(parents=True)
    (kit / "blocks.toml").write_text("[profiles.default]\n", encoding="utf-8")
    block = ('---\nname: %s\ndescription: "Says hello."\nmetadata:\n  kind: rule\n---\n\n## Rules\n- Say hello.\n\n'
             "## Check (script)\n- none: no output\n\n## Check (judge)\n- none: no output\n")
    with patch.object(block_resolver, "BLOCKS", kit):
        (kit / "a" / "SKILL.md").write_text(block % "a", encoding="utf-8")
        text = implement_run.resolve_blocks(Path(tmp), {"build": ["a"]}, "default")[0]["build"][1]
        assert "Concept: Says hello." in text and "---" not in text and "kind: rule" not in text, text
        (kit / "a" / "SKILL.md").write_text(block % "b", encoding="utf-8")
        try:
            implement_run.resolve_blocks(Path(tmp), {"build": ["a"]}, "default")
            raise AssertionError("a name that is not the id must stop start")
        except SystemExit as error:
            assert str(error) == "block a has the name b; a block's name is its id", error
        # A frontmatter that opens and does not close, or that has no name or no description, stops start,
        # so its lines never reach the step as header text.
        for broken, reason in (("---\nname: a\ndescription: \"D.\"\n\n## Rules\n", "no closing --- line"),
                               ("---\nname: a\ndescription: \"D.\"\n--- \n\n## Rules\n", "no closing --- line"),
                               ("---\ndescription: \"D.\"\n---\n\n## Rules\n", "no name"),
                               ("---\nname: a\n---\n\n## Rules\n", "no description"),
                               ("---\n---\n\n## Rules\n", "no name"),
                               ("---\nname: a\ndescription: \"D.\"\n\n## Rules\n- x\n\n---\n\n", "no closing --- line")):
            (kit / "a" / "SKILL.md").write_text(broken + "- x\n\n## Check (script)\n- none: x\n\n## Check (judge)\n- none: x\n",
                                                 encoding="utf-8")
            try:
                implement_run.resolve_blocks(Path(tmp), {"build": ["a"]}, "default")
                raise AssertionError(f"start must stop: {reason}")
            except SystemExit as error:
                assert f"block a frontmatter has {reason}" in str(error), error


def test_add_folder_block(tmp):
    """A repo block added as `<folder>/SKILL.md` takes the folder name as its id, as a kit block does."""
    Path(tmp, "add-folder", "app", "docs", "blocks", "foo").mkdir(parents=True)
    Path(tmp, "add-folder", "app", "docs", "blocks", "foo", "SKILL.md").write_text(
        '---\nname: foo\ndescription: "Foo rule."\n---\n\n## Rules\n- FOO RULE\n\n## Check (script)\n- none: x\n\n'
        "## Check (judge)\n- none: x\n", encoding="utf-8")
    out, folder, _, _ = blocks_start(tmp, "add-folder", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/blocks/foo/SKILL.md"]\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert "===== block:foo =====" in build and "Concept: Foo rule." in build, build
    # A SKILL.md with no plain folder name above it has no id.
    for path in ("SKILL.md", "docs/blocks/foo/../SKILL.md"):
        out, _, _, _ = blocks_start(tmp, f"add-folder-{len(path)}", PROVE_BUILD, PARAMS + f'[add]\nbuild = ["{path}"]\n')
        assert out.returncode == 1 and "a SKILL.md block sits in a folder named for its id" in out.stderr, out.stderr


def test_kit_pointers():
    """The kit installs as one plugin copy, so a skill reaches a block by a path from the kit root. Every
    `blocks/<id>/...` path in the kit's Markdown names a real file, and each skill that uses a kit path says
    where the kit root is. The kit ships two equal manifests, the Agent Plugins root `plugin.json` and
    Claude Code's `.claude-plugin/plugin.json`, so a version bump must change both."""
    kit = implement_run.BLOCKS.parent
    assert (kit / "plugin.json").read_bytes() == (kit / ".claude-plugin" / "plugin.json").read_bytes(), "the two manifests differ"
    for path in sorted(kit.glob("**/*.md")):
        if "vendor" in path.parts:
            continue
        text = path.read_text(encoding="utf-8")
        for target in re.findall(r"`(blocks/[a-z0-9-]+/[^`\s]+)`", text):
            assert (kit / target).is_file(), f"{path.relative_to(kit)} names {target}, which does not exist"
    for path in sorted(kit.glob("skills/*/SKILL.md")):
        text = path.read_text(encoding="utf-8")
        if "`blocks/" in text or "the kit's `references/" in text or "<kit root>" in text:
            assert "two folders up from this" in text, f"{path.relative_to(kit)} uses kit paths but does not say where the kit root is"


def test_kit_block_frontmatter():
    """Every kit block opens with Agent Skills frontmatter: name (its folder), description, license, and the
    metadata keys that blocks/blocks.toml names, each as `key: value`; only conflicts may be empty. The
    description says what the block does and when to use it ("Use when"), in 40 words or fewer, so a small
    model can pick it."""
    keys = ["kind", "owns", "serves", "needs", "gives", "conflicts", "users", "source"]
    for path in sorted(implement_run.BLOCKS.glob("*/SKILL.md")):
        text = path.read_text(encoding="utf-8")
        assert text.startswith("---\n"), f"{path} has no frontmatter"
        lines = text.split("---\n")[1].splitlines()
        top = [line.split(":", 1)[0] for line in lines if line[:1] != " "]
        meta = [line.strip().split(":", 1)[0] for line in lines if line[:1] == " "]
        parent = "  kind: parent" in lines
        assert top == ["name", "description", "license", "metadata"] and meta == keys + (["parts"] if parent else []), (path, top, meta)
        assert lines[0] == f"name: {path.parent.name}" and all(": " in line for line in lines if line != "metadata:"), path
        assert all(line.split(": ", 1)[1].strip() not in ("", '""', "''") for line in lines
                   if ": " in line and line.strip().split(":")[0] != "conflicts"), path
        description = lines[1].split(": ", 1)[1].strip('"')
        assert "Use when" in description and len(description.split()) <= 40, (path.parent.name, description)


def test_parent_selection(tmp):
    all_parts = ["when-to-diagram", "html-diagram", "call-tree", "diagram-check"]
    for index, (selected, expected) in enumerate((("diagram", all_parts), ("html-diagram", ["html-diagram"]),
                                                ("when-to-diagram, block:html-diagram", all_parts[:2]),
                                                ("html-diagram, block:diagram", all_parts),
                                                ("diagram, block:html-diagram", all_parts))):
        out, folder, _, _ = blocks_start(tmp, f"parent-{index}",
                                         f"- design: block:{selected}\n")
        assert out.returncode == 0, out.stderr
        text = step_text(folder, "02-design")
        assert text.count("===== block:diagram =====") == 1, text
        assert text.count("block:diagram@") == 1, text
        assert text.index("===== block:diagram =====") < min(text.index(f"===== block:{p} =====") for p in expected)
        for part in ("when-to-diagram", "html-diagram", "call-tree", "diagram-check"):
            assert text.count(f"===== block:{part} =====") == int(part in expected), text
        assert "block diagram judge:" in step_text(folder, "06-review")


def test_parent_refusals(tmp):
    root = Path(tmp, "parent-refusals")
    root.mkdir()
    (root / ".blocks.toml").write_text(PARAMS, encoding="utf-8")
    source = implement_run.BLOCKS
    cases = [
        ("unknown", {"diagram": ("parent", "missing")}, "unknown part missing"),
        ("two-parents", {"diagram": ("parent", "html-diagram"), "other": ("parent", "html-diagram")}, "two parents"),
        ("nested", {"diagram": ("parent", "other"), "other": ("parent", "html-diagram")}, "parent other is a part"),
        ("self", {"diagram": ("parent", "diagram")}, "parent diagram is a part"),
        ("not-parent", {"diagram": ("rule", "html-diagram")}, "parts is only allowed on kind: parent"),
        ("empty", {"diagram": ("parent", "")}, "parent diagram has no parts"),
    ]
    for name, changes, error in cases:
        blocks = root / name
        shutil.copytree(source, blocks)
        for block, (kind, parts) in changes.items():
            path = blocks / block / "SKILL.md"
            path.parent.mkdir(exist_ok=True)
            text = (source / "html-diagram" / "SKILL.md").read_text(encoding="utf-8")
            text = text.replace("name: html-diagram", f"name: {block}").replace("  kind: rule", f'  kind: {kind}\n  parts: "{parts}"')
            path.write_text(text, encoding="utf-8")
        with patch.object(block_resolver, "BLOCKS", blocks):
            try:
                implement_run.resolve_blocks(root, {"design": ["html-diagram"]}, "default")
            except SystemExit as caught:
                assert error in str(caught), (name, caught)
            else:
                raise AssertionError(f"{name} was accepted")


def test_diagram_membership():
    members, parents = {}, set()
    for path in sorted(implement_run.BLOCKS.glob("*/SKILL.md")):
        text = path.read_text(encoding="utf-8").split("---\n")[1]
        if line := re.search(r'^  parts: "(.*)"$', text, re.M):
            assert "  kind: parent" in text, path
            parents.add(path.parent.name)
            for part in line[1].split(","):
                members.setdefault(part.strip(), []).append(path.parent.name)
    for part, owners in members.items():
        assert len(owners) == 1 and part not in parents, (part, owners)
        assert (implement_run.BLOCKS / part / "SKILL.md").is_file(), part
    for part in ("when-to-diagram", "html-diagram", "call-tree", "diagram-check"):
        assert members.get(part) == ["diagram"], (part, members)


def shared_rule_copies(blocks):
    """The first seven words of each sentence in a parent's Rules are its key phrase."""
    copies = []
    for path in sorted(blocks.glob("*/SKILL.md")):
        text = path.read_text(encoding="utf-8")
        if not (members := re.search(r'^  parts: "(.*)"$', text, re.M)):
            continue
        rules = next(lines for head, lines in block_resolver.sections(text) if head == "Rules")
        for rule in (line[2:] for line in rules if line.startswith("- ")):
            for sentence in re.split(r"(?<=[.!?])\s+", rule):
                phrase = " ".join(re.findall(r"[a-z0-9]+", sentence.lower())[:7])
                for part in members[1].split(","):
                    body = (blocks / part.strip() / "SKILL.md").read_text(encoding="utf-8").split("---\n", 2)[-1]
                    if phrase in " ".join(re.findall(r"[a-z0-9]+", body.lower())):
                        copies.append((part.strip(), phrase))
    return copies


def test_shared_rules_not_copied(tmp):
    assert not shared_rule_copies(implement_run.BLOCKS), shared_rule_copies(implement_run.BLOCKS)
    blocks = Path(tmp, "copied-rule")
    shutil.copytree(implement_run.BLOCKS, blocks)
    parent = (blocks / "diagram" / "SKILL.md").read_text(encoding="utf-8")
    part = blocks / "html-diagram" / "SKILL.md"
    original = part.read_text(encoding="utf-8")
    for rule in next(lines for head, lines in block_resolver.sections(parent) if head == "Rules"):
        if rule.startswith("- "):
            for sentence in re.split(r"(?<=[.!?])\s+", rule[2:]):
                part.write_text(original + "\n- " + sentence + "\n", encoding="utf-8")
                assert shared_rule_copies(blocks), f"the check accepted a copied parent sentence: {sentence}"


def test_kit_blocks_load(tmp):
    """Every kit block loads, so each has both check sections and its params are the documented ones."""
    root = Path(tmp, "kit-blocks")
    root.mkdir()
    (root / ".blocks.toml").write_text(PARAMS, encoding="utf-8")
    ids = [path.parent.name for path in sorted(implement_run.BLOCKS.glob("*/SKILL.md"))]
    out, _, _ = implement_run.resolve_blocks(root, {"build": ids}, "default")
    # A block folder that lost its SKILL.md would be skipped by the glob in silence, so every folder counts.
    folders = [path.name for path in implement_run.BLOCKS.iterdir() if path.is_dir() and path.name != "__pycache__"]
    assert len(ids) == len(folders) and all(f"===== block:{b} =====" in out["build"][1] for b in ids), (ids, out["build"][0])


def test_block_repo_file(tmp):
    """The repo's [add] puts its own block into a step, and an [off] entry with a reason drops a block
    and says so in the step it would have joined."""
    Path(tmp, "repo-file", "app", "docs").mkdir(parents=True)
    Path(tmp, "repo-file", "app", "docs", "our-api.md").write_text(
        "# our-api\nConcept: x\n\n## Rules\n- OUR RULE 7\n\n## Check (script)\n- `true` runs: it proves nothing\n\n## Check (judge)\n", encoding="utf-8")
    out, folder, _, _ = blocks_start(tmp, "repo-file", "- design: block:tech-diagram\n",
                                     PARAMS + '[add]\nbuild = ["docs/our-api.md"]\n04-style-frames = ["docs/visual.md"]\n[off]\nhtml-diagram = "this repo\\u001b draws\\nSVG"\n')
    assert out.returncode == 0, out.stderr
    build = step_text(folder, "05-build")
    assert "===== block:our-api =====" in build and "OUR RULE 7" in build and "- [ ] block our-api script: `true` runs: it proves nothing: " in build, build
    design = step_text(folder, "02-design")
    assert "Off: html-diagram: this repo draws SVG" in design and "===== block:html-diagram" not in design, design
    assert "block html-diagram" not in design and "block html-diagram" not in step_text(folder, "06-review")


def test_block_cap(tmp):
    """A block of CAP characters loads; a longer one stops start with one line, since a cut could end inside a
    check and drop the checks after it."""
    head, tail = "# big\n## Rules\n", "## Check (script)\n- `true` runs: BIG CHECK\n## Check (judge)\n- Is it big?\n"
    for name, size in (("big-ok", implement_run.CAP), ("big", implement_run.CAP + 1)):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        pad = "- " + "x" * (size - len(head) - len(tail) - 3) + "\n"
        Path(tmp, name, "app", "docs", "big.md").write_text(head + pad + tail, encoding="utf-8")
        out, folder, repo, _ = blocks_start(tmp, name, PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/big.md"]\n')
        if name == "big-ok":
            assert out.returncode == 0 and "BIG CHECK" in step_text(folder, "05-build"), out.stderr
        else:
            assert out.returncode == 1 and out.stderr == f"block big is {size} characters; a block holds at most {implement_run.CAP}\n", out.stderr
            assert not (repo / "tmp" / "flow").exists()
