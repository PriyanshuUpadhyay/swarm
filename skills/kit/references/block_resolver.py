"""Shared block membership, selection, validation, and packing for kit workflows."""

import hashlib
import re
import shlex
import sys
from pathlib import Path

from block_ledger import NOT_NAMED_REASON, ledger_markdown
from step_run import plain

BLOCKS = Path(__file__).resolve().parents[1] / "blocks"
CAP = 15000  # skills are cut to this size; blocks over it are refused, since a cut could stop inside a check
PARAM = re.compile(r"\{\{(\w+)\}\}")
CHECKS = ("Check (script)", "Check (judge)")


def kit_profile(kit, kit_file, name):
    if (profile := kit.get("profiles", {}).get(name)) is None:
        raise SystemExit(f"unknown profile {name}; {kit_file} has {', '.join(kit.get('profiles', {}))}")
    return profile


def resolve_blocks(root, wanted, profile_name, *, blocks_dir=None, pack=True, params_override=None, judge_todos=False):
    """For each step key in `wanted` (key -> catalog block ids), the blocks it gets from the catalog and
    the repo's .blocks.toml, packed by the profile: (Skills read items, rules text, script todos). Also
    the judge lines of every block that ran, once each, and the blocks table. blocks/blocks.toml owns the format.
    With pack=False, return only the selected ids, without base rules or filled content. A root of None
    reads only the kit catalog, as a plan does before a repo run exists."""
    blocks_dir = BLOCKS if blocks_dir is None else blocks_dir
    kit_file = blocks_dir / "blocks.toml"
    repo_file = root / ".blocks.toml" if root is not None else None
    # A kit copy without blocks can still run; a shipped catalog is read even when all blocks are rejected.
    if not kit_file.exists() and profile_name == "default" and not (repo_file and repo_file.exists()) and not any(wanted.values()):
        if not pack:
            return {key: [] for key in wanted}
        return {key: ([], "", []) for key in wanted}, "", ledger_markdown({}, {})
    kit = read_toml(kit_file)
    files = {path.parent.name: path for path in sorted(blocks_dir.glob("*/SKILL.md"))}
    aliases = kit.get("aliases", {})
    for alias, members in aliases.items():
        if alias in files:
            raise SystemExit(f"alias {alias} is also a block id in {kit_file}; rename one")
        if unknown := [m for m in members if m not in files]:
            raise SystemExit(f"unknown block id {unknown[0]} in alias {alias} of {kit_file}")
    if unknown := [b for b in kit.get("base", []) if b not in files]:
        raise SystemExit(f"unknown block id {unknown[0]} in base of {kit_file}")
    profile = kit_profile(kit, kit_file, profile_name)
    merge = profile.get("merge", [])
    if unknown := [name for name in merge if name not in aliases]:
        raise SystemExit(f"profile {profile_name} merges {unknown[0]}, which is not an alias in {kit_file}")

    repo = read_toml(repo_file) if repo_file and repo_file.exists() else {}
    if unknown := sorted(set(repo) - {"params", "add", "off"}):
        raise SystemExit(f"unknown table [{unknown[0]}] in {repo_file}; it takes [params], [add], and [off]")
    if wrong := [t for t in ("params", "add", "off") if not isinstance(repo.get(t, {}), dict)]:
        raise SystemExit(f"{repo_file}: {wrong[0]} must be a table, written as [{wrong[0]}]")
    params, off, added = {**repo.get("params", {}), **(params_override or {})}, repo.get("off", {}), {}
    if bad := next((k for k, v in params.items() if not isinstance(v, str)), None):
        raise SystemExit(f"[params] {bad} must be a string in {repo_file}, such as {bad} = \"docs/x.md\"")
    # An empty value would reach a script check as '', which a command reads as a missing path.
    if bad := next((k for k, v in params.items() if not one_line(v).strip()), None):
        raise SystemExit(f"[params] {bad} is empty in {repo_file}; a value is a path or a name, such as {bad} = \".\"")
    # A value read as an option would change a block command, and one that starts a line with `## ` would put a
    # step heading (`## Todo (`, `## Result`) into the rules text, where done would take it for the todo list.
    if bad := next((k for k, v in params.items() if one_line(str(v)).lstrip()[:1] in ("-", "#")), None):
        raise SystemExit(f"[params] {bad} starts with {one_line(str(params[bad])).lstrip()[0]!r} in {repo_file}; "
                         "a value is a path or a name, never an option or a heading")
    # A backtick would end a script check's code span early, so the todo would not hold the command as written.
    if bad := next((k for k, v in params.items() if "`" in str(v)), None):
        raise SystemExit(f"[params] {bad} holds a backtick in {repo_file}; a value is a path or a name")
    for key, paths in repo.get("add", {}).items():
        if key not in wanted:
            # Another skill's step key is valid here, but a misspelled key must leave a trace.
            print(f"[add] {key} in {repo_file} names no step of this skill; skipped", file=sys.stderr)
            continue
        if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths):
            raise SystemExit(f"[add] {key} in {repo_file} must be a list of paths, such as {key} = [\"docs/x.md\"]")
        for path in paths:
            if "\0" in path:
                raise SystemExit(f"[add] {path} in {repo_file} holds a NUL byte")
            try:  # Python 3.11 and 3.12 raise for a symlink loop (RuntimeError) or a too-long name (OSError)
                outside = Path(path).is_absolute() or not (root / path).resolve().is_relative_to(root.resolve())
                is_file = (root / path).is_file()
            except (OSError, RuntimeError):
                raise SystemExit(f"[add] {path} in {repo_file} is not a file") from None
            # The repo file is repo-owned input, and the step text goes to agents, so no file outside the repo.
            if outside:
                raise SystemExit(f"[add] {path} leaves the repo; {repo_file} takes repo-relative paths")
            # A repo block in the kit's layout, <folder>/SKILL.md, takes the folder name as its id.
            block = Path(path).parent.name if Path(path).name == "SKILL.md" else Path(path).stem
            if block in ("", ".", ".."):
                raise SystemExit(f"[add] {path} in {repo_file} has no id; a SKILL.md block sits in a folder named for its id")
            # The id goes into the step file and take's output, so it is one plain word.
            if not re.fullmatch(r"[A-Za-z0-9._-]+", block):
                raise SystemExit(f"[add] {path} in {repo_file} has id {block}, which is not only letters, digits, "
                                 "dots, dashes, and underscores; rename the file")
            if block in aliases:
                raise SystemExit(f"[add] {path} in {repo_file} has id {block}, which is an alias in {kit_file}; rename the file")
            if files.get(block, root / path) != root / path:
                raise SystemExit(f"duplicate block id {block}: {path} in {repo_file} and {files[block]}")
            if not is_file:
                raise SystemExit(f"[add] {path} in {repo_file} is not a file")
            files[block] = root / path
            added.setdefault(key, []).append(block)
    # ponytail: off works per block; per-check off (block id plus check line) is the upgrade if a repo needs it.
    for block, reason in off.items():
        if block not in files:
            raise SystemExit(f"unknown block id {block} in [off] of {repo_file}")
        if not isinstance(reason, str) or not one_line(reason).strip():  # the text that the Off line prints
            raise SystemExit(f"[off] {block} needs a reason in {repo_file}")

    # Membership has one owner: a parent's metadata.parts, never a field on its parts.
    parents, parent_of = {}, {}
    for block, path in files.items():
        text = read_utf8(path)
        head = text[3:].partition("\n---\n")[0] if text.startswith("---\n") else ""
        meta = dict(re.findall(r"^  (kind|parts): (.*)$", head, re.M))
        meta = {k: v.strip().strip("\"'") for k, v in meta.items()}
        if "parts" in meta and meta.get("kind") != "parent":
            raise SystemExit(f"block {block}: parts is only allowed on kind: parent")
        if meta.get("kind") == "parent":
            parts = [p.strip() for p in meta.get("parts", "").split(",") if p.strip()]
            if not parts:
                raise SystemExit(f"parent {block} has no parts")
            parents[block] = parts
    for parent, parts in parents.items():
        for part in parts:
            if part not in files:
                raise SystemExit(f"unknown part {part} in parent {parent}")
            if part in parents:
                raise SystemExit(f"parent {part} is a part of parent {parent}; parents have one level")
            if part in parent_of:
                raise SystemExit(f"part {part} has two parents: {parent_of[part]} and {parent}")
            parent_of[part] = parent

    named = {}
    for key, ids in wanted.items():
        if unknown := [i for i in ids if i not in files and i not in aliases]:
            raise SystemExit(f"unknown block id {unknown[0]} in step {key}")
        named[key] = [m for i in ids for m in aliases.get(i, [i])] + added.get(key, [])
    base = kit.get("base", []) if pack and any(named.values()) else []  # a run that names no block stays as before
    # The base block needs source_of_truth, which only the repo's .blocks.toml sets. A repo with no such file
    # still runs the blocks that its catalog lines name, such as an `any` line, and the step says the base is off.
    if base and not (repo_file and repo_file.exists()):
        off = {b: "the repo has no .blocks.toml, which sets source_of_truth" for b in base}  # no file, no [off]

    selected = {}
    for key, ids in named.items():
        expanded = []
        for block in base + ids:
            if block in parent_of:
                expanded.append(parent_of[block])
            expanded.append(block)
            expanded.extend(parents.get(block, []))
        selected[key] = list(dict.fromkeys(expanded))
    if not pack:
        return {key: [b for b in ids if b not in off] for key, ids in selected.items()}

    loaded = {}

    def load(block):
        """The block's content hash and its filled sections, read once."""
        if block not in loaded:
            data = read_bytes(files[block])
            # Repo text goes to agents and to take's output: a param is one line, and no text keeps control bytes.
            # The param check reads the filtered text, the same text the fill reads.
            text = plain(read_utf8(files[block], data), keep_newlines=True)
            # A large repo file would fill every step file, and a cut could end inside a check, so it stops start.
            if len(text) > CAP:
                raise SystemExit(f"block {block} is {len(text)} characters; a block holds at most {CAP}")
            # Agent Skills frontmatter: only name and description are read, as one-line values in plain or
            # double quotes; YAML escapes are not decoded. The name is the block id, and the description goes
            # to the step as the Concept line, in place of the frontmatter.
            if text.startswith("---\n"):
                # Keep the opening newline, so an empty frontmatter finds its close. A `## ` line before the
                # close means the frontmatter never closed and a later `---` rule in the body was taken for it.
                head, close, rest = text[3:].partition("\n---\n")
                if not close or "\n## " in head:
                    raise SystemExit(f"block {block} frontmatter has no closing --- line")
                fields = dict(l.split(": ", 1) for l in head.splitlines() if ": " in l and l[:1] != " ")
                fields = {key: value.strip().strip('"') for key, value in fields.items()}
                for key in ("name", "description"):
                    if not fields.get(key):
                        raise SystemExit(f"block {block} frontmatter has no {key}")
                if fields["name"] != block:
                    raise SystemExit(f"block {block} has the name {fields['name']}; a block's name is its id")
                text = f"Concept: {fields['description']}\n" + rest
            if missing := [p for p in PARAM.findall(text) if p not in params]:
                raise SystemExit(f"block {block} needs {{{{{missing[0]}}}}}; set it under [params] in {repo_file}")
            def fill(part, quote=False):
                return PARAM.sub(lambda m: (shlex.quote if quote else str)(one_line(str(params[m[1]]))), part)

            # A script check runs its backtick spans in a shell, so a value there is one quoted word; the rest of
            # the line, and other sections, get the value as written. Odd split parts are the spans.
            parts = [(head, ["".join(fill(p, head == CHECKS[0] and i % 2) for i, p in enumerate(re.split(r"(`[^`]*`)", line)))
                             for line in lines]) for head, lines in sections(text)]
            # sections() split the raw text, so a `## ` line here was made by a param; in the step file it
            # would be a step heading, such as `## Todo (` or `## Result`, that done takes for the todo list.
            if made := next((l for _, lines in parts for l in lines if l.startswith("## ")), None):
                raise SystemExit(f"block {block} has the line {made!r} once its params are filled; "
                                 f"a param must not make a '## ' heading")
            # A misspelled check heading would drop its checks without a word.
            if bad := [h for h, _ in parts[1:] if h.lower().startswith("check") and h not in CHECKS]:
                raise SystemExit(f"block {block} has the section heading ## {bad[0]}; a check section is "
                                 f"## {CHECKS[0]} or ## {CHECKS[1]}")
            # So would a check line that is not a `- ` item, as `* ` or an indented `- ` is.
            if bad := next(((h, l) for h, lines in parts[1:] if h in CHECKS for l in lines
                            if l.strip() and not l.startswith("- ")), None):
                raise SystemExit(f"block {block} has the line {bad[1]!r} under ## {bad[0]}; "
                                 f"each check is one line that starts with '- '")
            # So would a missing check section; blocks/blocks.toml says every block has both.
            if missing := next((h for h in CHECKS if h not in [head for head, _ in parts[1:]]), None):
                raise SystemExit(f"block {block} has no ## {missing} section; every block has one, "
                                 "with `- none: <why>` when it has no check")
            # The hash is of the file's bytes; the text has LF for CRLF, so it cannot give it.
            loaded[block] = (hashlib.sha256(data).hexdigest()[:8], parts)
        return loaded[block]

    out, judge = {}, {}
    for key, joined in selected.items():
        blocks = [b for b in joined if b not in off]
        groups = {}  # a merged alias sits where its first member stood
        for b in blocks:
            groups.setdefault(next((a for a in merge if b in aliases[a]), b), []).append(b)
        rest = [g for g in groups if g not in base]
        if profile.get("merge_base") and rest:
            folded = [b for g in groups if g in base for b in groups[g]]
            groups = {g: folded + groups[g] if g == rest[0] else groups[g] for g in rest}
        text = "\n".join([f"Off: {b}: {one_line(str(off[b]))}" for b in joined if b in off]
                         + [render(name, [load(b)[1] for b in members], profile.get("examples", True))
                            for name, members in groups.items()])
        read = [f"profile {profile_name}"] + [f"block:{b}@{load(b)[0]}" for b in blocks] if blocks else []
        todos = [f"block {b} script: {line}" for b in blocks for line in check_lines(load(b)[1], "Check (script)")]
        if judge_todos:
            todos += [f"block {b} judge: {line}" for b in blocks for line in check_lines(load(b)[1], "Check (judge)")]
        for b in blocks:
            judge.setdefault(b, check_lines(load(b)[1], "Check (judge)"))
        out[key] = (read, text, todos)
    ledger = {}
    for block in files:
        if block in kit.get("base", []):
            continue
        steps = [key for key, ids in named.items() if block in ids and block not in off]
        if steps:
            ledger[block] = ("Chosen", f"Loaded by {', '.join(steps)}")
        else:
            reason = f"[off] {one_line(off[block])}" if block in off else NOT_NAMED_REASON
            ledger[block] = ("Rejected", reason)
    return out, "\n".join(f"- block {b} judge: {line}" for b, lines in judge.items() for line in lines), ledger_markdown(ledger, aliases)


def one_line(text):
    return plain(text, keep_newlines=True).replace("\n", " ").replace("\t", " ")


def read_toml(path):
    try:  # import when needed, so commands that do not read TOML still run on Python before 3.11
        import tomllib
    except ImportError:
        raise SystemExit(f"{path}: reading TOML needs Python 3.11 or later (tomllib); use a newer python3, for example from Homebrew") from None
    try:
        return tomllib.loads(read_utf8(path))
    except (tomllib.TOMLDecodeError, RecursionError, ValueError) as error:  # a deep array, a huge integer
        raise SystemExit(f"{path} is not valid TOML: {error}") from None


def read_bytes(path):
    try:
        return path.read_bytes()
    except OSError as error:
        raise SystemExit(f"{path} cannot be read: {error.strerror}") from None


def read_utf8(path, data=None):
    """The file's text with each line end as LF, as read_text gives it; `data` is its bytes if already read."""
    try:
        return (read_bytes(path) if data is None else data).decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")
    except UnicodeDecodeError as error:
        raise SystemExit(f"{path} is not UTF-8: {error}") from None


def sections(text):
    """A block's header lines and each `## ` section, in order: [(None, header), (heading, lines), ...]."""
    parts = [(None, [])]
    for line in text.splitlines():
        if line.startswith("## "):
            parts.append((line[3:].strip(), []))
        else:
            parts[-1][1].append(line)
    return parts


def check_lines(parts, heading):
    return [line[2:] for head, lines in parts if head == heading for line in lines
            if line.startswith("- ") and not line.startswith("- none:")]


def render(name, members, examples):
    """Blocks as step text under one name, without their check sections, which become todos and judge
    lines. One member is the block itself; several are one merged block."""
    lines = [f"===== block:{name} =====", f"# {name}"]
    lines += [line for parts in members for line in parts[0][1] if not line.startswith("# ")]
    body = {}
    for parts in members:
        for head, section in parts[1:]:
            if head not in CHECKS and (examples or head != "Example"):
                body.setdefault(head, []).extend(section)
    # `### ` nests each section under the block marker. sections() split on every `## ` line, so no block
    # line now starts with `## `, and repo text cannot stand in for the step's todo head or `## Result`.
    return "\n".join(lines + [line for head, section in body.items() for line in (f"### {head}", *section)]).rstrip() + "\n"


