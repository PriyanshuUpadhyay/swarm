# Implement test block errors cases and helpers.
from test_implement_run_support import *


def test_block_fail_closed(tmp):
    """start stops with a one-line reason and makes no folder."""
    cases = [
        ("unknown", "- build: block:nope\n", PARAMS, (), "unknown block id nope"),
        ("off-empty", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = " "\n', (), "[off] html-diagram needs a reason"),
        ("off-list", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = []\n', (), "[off] html-diagram needs a reason"),
        ("off-table", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = {}\n', (), "[off] html-diagram needs a reason"),
        ("off-number", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = 3\n', (), "[off] html-diagram needs a reason"),
        ("off-false", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = false\n', (), "[off] html-diagram needs a reason"),
        # The Off line prints the reason through one_line, so control characters alone are an empty reason.
        ("off-controls", "- design: block:tech-diagram\n", PARAMS + '[off]\nhtml-diagram = "\\u001b\\u0007"\n', (), "[off] html-diagram needs a reason"),
        ("off-unknown", "- design: block:tech-diagram\n", PARAMS + '[off]\nnope = "x"\n', (), "unknown block id nope"),
        ("duplicate", PROVE_BUILD, PARAMS + '[add]\nbuild = ["prove-it-works.md"]\n', (), "duplicate block id prove-it-works"),
        ("profile", PROVE_BUILD, PARAMS, ("--profile", "nope"), "unknown profile nope"),
    ]
    Path(tmp, "duplicate", "app").mkdir(parents=True)
    Path(tmp, "duplicate", "app", "prove-it-works.md").write_text("# prove-it-works\n", encoding="utf-8")
    # An [add] path must stay inside the repo: absolute, `..`, and a symlink out are refused.
    for name, path in (("add-absolute", str(Path(tmp, "add-absolute", "outside.md"))),
                       ("add-parent", "../outside.md"), ("add-symlink", "link.md")):
        Path(tmp, name, "app").mkdir(parents=True)
        Path(tmp, name, "outside.md").write_text("# outside\nSECRET\n", encoding="utf-8")
        cases.append((name, PROVE_BUILD, PARAMS + f'[add]\nbuild = ["{path}"]\n', (),
                      f"[add] {path} leaves the repo"))
    Path(tmp, "add-symlink", "app", "link.md").symlink_to(Path(tmp, "add-symlink", "outside.md"))
    # A repo block whose id equals a kit alias name would mix with the alias.
    Path(tmp, "add-alias", "app", "docs").mkdir(parents=True)
    Path(tmp, "add-alias", "app", "docs", "tech-diagram.md").write_text("# tech-diagram\n", encoding="utf-8")
    cases.append(("add-alias", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/tech-diagram.md"]\n', (),
                  "has id tech-diagram, which is an alias"))
    # A TOML syntax error or a wrong value type in the repo file names the file, with no traceback.
    # A top-level key goes before [params], or TOML puts it in that table.
    cases += [(f"type-{name}", PROVE_BUILD, toml, (), reason) for name, toml, reason in (
        ("syntax", "[params\nx = 1\n", ".blocks.toml is not valid TOML"),
        ("deep", "x = " + "[" * 500 + "]" * 500 + "\n" + PARAMS, ".blocks.toml is not valid TOML"),  # RecursionError
        ("bigint", PARAMS + "big = " + "9" * 5000 + "\n", ".blocks.toml is not valid TOML"),  # ValueError, int digit limit
        ("params", "params = 3\n", ".blocks.toml: params must be a table"),
        ("off", "off = 3\n" + PARAMS, ".blocks.toml: off must be a table"),
        ("add", "add = 3\n" + PARAMS, ".blocks.toml: add must be a table"),
        ("add-string", PARAMS + '[add]\nbuild = "docs/x.md"\n', "must be a list of paths"))]
    cases += [
        ("add-missing", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/none.md"]\n', (), "[add] docs/none.md in"),
        ("table", PROVE_BUILD, PARAMS + "[extra]\nx = 1\n", (), "unknown table [extra]"),
        ("param-dash", PROVE_BUILD, PARAMS.replace('site = "site"', 'site = "--version"'), (), "[params] site starts with '-'"),
        ("param-dash-space", PROVE_BUILD, PARAMS.replace('site = "site"', 'site = "\\n -x"'), (), "[params] site starts with '-'"),
        ("param-backtick", PROVE_BUILD, PARAMS.replace('site = "site"', 'site = "x`; echo PWNED; `y"'), (), "[params] site holds a backtick"),
        # An empty value would reach a script check as '', which rg reads as a missing path.
        ("param-empty", PROVE_BUILD, PARAMS.replace('site = "site"', 'site = ""'), (), "[params] site is empty"),
        ("param-blank", PROVE_BUILD, PARAMS.replace('site = "site"', 'site = " \\u001b "'), (), "[params] site is empty"),
        # A value that is not a string would reach a block sentence as a Python repr, such as ['a', 'b'] or True.
        *[(f"param-{kind}", PROVE_BUILD, PARAMS.replace('site = "site"', f"site = {value}"), (), "[params] site must be a string")
          for kind, value in (("list", '["a", "b"]'), ("bool", "true"), ("number", "3"), ("table", "{ a = 1 }"))],
    ]
    # Bytes that are not UTF-8 in the repo file or in an [add] block name the file, with no traceback.
    Path(tmp, "utf8-toml", "app").mkdir(parents=True)
    Path(tmp, "utf8-toml", "app", ".blocks.toml").write_bytes(PARAMS.encode() + b'x = "\xff"\n')
    cases.append(("utf8-toml", PROVE_BUILD, None, (), ".blocks.toml is not UTF-8"))
    Path(tmp, "utf8-add", "app", "docs").mkdir(parents=True)
    Path(tmp, "utf8-add", "app", "docs", "x.md").write_bytes(b"# x\n\xff\n")
    cases.append(("utf8-add", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/x.md"]\n', (), "x.md is not UTF-8"))
    # A repo file that cannot be read, an [add] path with a NUL byte, and control characters in repo
    # text each give one clean line.
    Path(tmp, "toml-dir", "app", ".blocks.toml").mkdir(parents=True)
    cases += [
        ("toml-dir", PROVE_BUILD, None, (), ".blocks.toml cannot be read"),
        ("add-nul", PROVE_BUILD, PARAMS + '[add]\nbuild = ["a\\u0000b.md"]\n', (), "holds a NUL byte"),
        ("add-id-controls", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/x\\u001b[2Jy\\n- [ ] injected.md"]\n', (), "rename the file"),
        ("add-id-space", PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/my block.md"]\n', (), "has id my block, which is not only"),
        ("controls", PROVE_BUILD, PARAMS + '["\\u001b[2Jx\\ny"]\n', (), "unknown table [[2Jx y]"),
    ]
    # A catalog skill path that cannot be read, or is not UTF-8, stops start before any step file exists.
    Path(tmp, "skill-dir", "skill").mkdir(parents=True)
    Path(tmp, "skill-utf8").mkdir()
    Path(tmp, "skill-utf8", "skill.md").write_bytes(b"# skill\n\xff\n")
    cases += [
        ("skill-dir", f"- build: {Path(tmp, 'skill-dir', 'skill')}\n", PARAMS, (), "skill cannot be read"),
        ("skill-utf8", f"- build: {Path(tmp, 'skill-utf8', 'skill.md')}\n", PARAMS, (), "skill.md is not UTF-8"),
    ]
    # A block id comes from an [add] file name, so the files exist and only the name is wrong.
    for name, file in (("add-id-controls", "x\x1b[2Jy\n- [ ] injected.md"), ("add-id-space", "my block.md")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", file).write_text("# x\n\n## Rules\n- r\n", encoding="utf-8")
    # A misspelled check heading would drop the block's checks without a word, so start refuses it.
    for name, head in (("check-case", "Check (Script)"), ("check-space", "Check(script)")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", "c.md").write_text(f"# c\n\n## Rules\n- r\n\n## {head}\n- `true` runs: ok\n", encoding="utf-8")
        cases.append((name, PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/c.md"]\n', (), f"block c has the section heading ## {head}"))
    # A param value that starts with `#` would put a step heading, such as `## Todo (` or `## Result`, into rules text.
    Path(tmp, "param-heading", "app", "docs").mkdir(parents=True)
    Path(tmp, "param-heading", "app", "docs", "fake.md").write_text("# fake\n\n## Rules\n{{a}}\n- [x] nothing: ok\n{{b}}\n",
                                                                   encoding="utf-8")
    cases.append(("param-heading", PROVE_BUILD, PARAMS + 'a = "## Todo (x)"\nb = "## Result"\n[add]\ndesign = ["docs/fake.md"]\n', (),
                  "[params] a starts with '#'"))
    # A block line that a param fills into a `## ` line would also be a step heading in the rules text.
    Path(tmp, "param-filled-heading", "app", "docs").mkdir(parents=True)
    Path(tmp, "param-filled-heading", "app", "docs", "fake.md").write_text(
        "# fake\n\n## Rules\n##{{a}}\n- [x] nothing: ok\n##{{b}}\n", encoding="utf-8")
    cases.append(("param-filled-heading", PROVE_BUILD,
                  PARAMS + 'a = " Todo (x)"\nb = " Result"\n[add]\ndesign = ["docs/fake.md"]\n', (),
                  "block fake has the line '## Todo (x)' once its params are filled"))
    # A check line that is not a `- ` item would be dropped without a word too.
    for name, line in (("check-star", "* `true` runs: ok"), ("check-indent", "  - `false` runs: indented")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", "c.md").write_text(f"# c\n\n## Rules\n- r\n\n## Check (script)\n- `true` runs: ok\n{line}\n",
                                                          encoding="utf-8")
        cases.append((name, PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/c.md"]\n', (),
                      f"block c has the line {line!r} under ## Check (script)"))
    # Every block has both check sections; `- none: <why>` is the way to have no check, so a missing one stops start.
    for name, text, head in (("no-checks", "# c\n\n## Rules\n- r\n", "Check (script)"),
                             ("no-judge", "# c\n\n## Rules\n- r\n\n## Check (script)\n- none: no command\n", "Check (judge)")):
        Path(tmp, name, "app", "docs").mkdir(parents=True)
        Path(tmp, name, "app", "docs", "c.md").write_text(text, encoding="utf-8")
        cases.append((name, PROVE_BUILD, PARAMS + '[add]\nbuild = ["docs/c.md"]\n', (), f"block c has no ## {head} section"))
    for name, catalog, repo_toml, args, reason in cases:
        out, _, repo, _ = blocks_start(tmp, name, catalog, repo_toml, *args)
        assert out.returncode == 1 and reason in out.stderr and "Traceback" not in out.stderr, (name, out.stderr)
        assert len(out.stderr.strip().splitlines()) == 1, (name, out.stderr)
        assert not has_control(out.stderr.rstrip("\n"), keep=""), (name, out.stderr)
        assert not (repo / "tmp" / "flow").exists(), name
    # A bad block catalog stops the load with a reason that names it.
    kit = Path(tmp, "kit")
    kit.mkdir()
    (kit / "a").mkdir()
    (kit / "a" / "SKILL.md").write_text("# a\n", encoding="utf-8")
    for toml, reason in (('base = []\n[aliases]\na = ["a"]\n[profiles.default]\n', "alias a is also a block id"),
                         ("[aliases\n", "blocks.toml is not valid TOML"),
                         ('[aliases]\nb = ["nope"]\n[profiles.default]\n', "unknown block id nope in alias b"),
                         ('base = ["nope"]\n[profiles.default]\n', "unknown block id nope in base of"),
                         ('[profiles.default]\nmerge = ["a"]\n', "merges a, which is not an alias")):
        (kit / "blocks.toml").write_text(toml, encoding="utf-8")
        with patch.object(block_resolver, "BLOCKS", kit):
            try:
                implement_run.resolve_blocks(Path(tmp), {"build": ["a"]}, "default")
                raise AssertionError(f"start must stop: {reason}")
            except SystemExit as error:
                assert reason in str(error) and "\n" not in str(error), error
