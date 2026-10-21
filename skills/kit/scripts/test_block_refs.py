"""Run: python3 test_block_refs.py

Every block id that a kit file names resolves to a block folder, so a rename with no alias cannot leave a
caller pointing at nothing. It reads `blocks/<id>/` paths, `block:<id>` catalog names, the Block cell of
plan tables, `conflicts`, and the base and alias lists of blocks.toml.
"""
import re, tomllib
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]
BLOCKS = KIT / "blocks"
ROW = re.compile(r"^\|\s*`?\d\d-[a-z0-9-]+(?:\.md)?`?(?:\s*\(optional\))?\s*\|")


def refs(path, text):
    for n, line in enumerate(text.splitlines(), 1):
        for id in re.findall(r"(?<![\w.-])blocks/([a-z0-9][a-z0-9-]*)/", line):
            yield n, id
        for id in re.findall(r"\bblock:([a-z0-9][a-z0-9-]*)", line):
            yield n, id
        # | File | Needs | Holds | Block | Why |, or with Writes before Block
        if ROW.match(line) and len(cells := re.split(r"(?<!\\)\|", line.strip().strip("|"))) in (5, 6):
            yield from ((n, id.strip(" `")) for id in cells[len(cells) - 2].split(",") if id.strip(" `"))
        if m := re.match(r"\s+conflicts: \"?([^\"]*)\"?$", line):
            yield from ((n, id.strip()) for id in m[1].split(",") if id.strip())


def test_every_named_block_exists():
    catalog = tomllib.loads((BLOCKS / "blocks.toml").read_text(encoding="utf-8"))
    ids = {p.name for p in BLOCKS.iterdir() if (p / "SKILL.md").is_file()}
    known = ids | set(catalog["aliases"])
    bad = [f"blocks.toml: {id}" for id in catalog["base"] + sum(catalog["aliases"].values(), []) if id not in ids]
    for path in sorted(KIT.rglob("*")):
        if path.suffix not in (".md", ".py", ".toml", ".json") or {"vendor", "__pycache__"} & set(path.parts):
            continue
        if path.name.startswith("test_") and path.parent.parent.parent != BLOCKS:
            continue  # tests outside blocks/ name made-up blocks on purpose
        bad += [f"{path.relative_to(KIT)}:{n}: {id}" for n, id in refs(path, path.read_text(encoding="utf-8")) if id not in known]
    assert not bad, "names a block that does not exist:\n" + "\n".join(bad)


def test_the_scan_sees_each_form():
    text = ("see blocks/gone-a/SKILL.md\n- build: block:gone-b\n| 01-x | none | y | voice-reply, gone-c | z |\n  conflicts: \"gone-d\"\n"
            "| `02-y.md` (optional) | x | y | `src/` | gone-e | z |\n")
    assert [id for _, id in refs(Path("x.md"), text)] == ["gone-a", "gone-b", "voice-reply", "gone-c", "gone-d", "gone-e"]


if __name__ == "__main__":
    test_the_scan_sees_each_form()
    test_every_named_block_exists()
    print("ok")
