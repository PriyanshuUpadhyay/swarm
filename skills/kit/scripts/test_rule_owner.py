"""Run: python3 test_rule_owner.py

One file owns each rule; other kit files point to it. This fails when one sentence of 12 or more words
appears word for word in two or more Markdown files of the kit, so a rule cannot be copied into a second
file. It compares sentences after lower case, with Markdown markup and extra whitespace removed. A list
item and a table row count as their own sentence, also with no final stop. It skips evals/ and fixtures/,
because their planted files copy kit text on purpose.
ALLOWED holds the copies that existed when the check came in; it prints them on each run, and fails on an
entry that no longer occurs twice. Limit: it does not catch the same rule said again in new words.
"""
import re
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]
MIN_WORDS = 12
SHOW = 100  # characters of a sentence that a message shows
# Copies that existed when this check came in. Remove an entry once its copies point to the owner.
ALLOWED = {
    "pass an owner's saved line with review <folder <range continue <user file .",
    "run swarm board for the flags; a refused post prints how to fix it.",
    "the kit root, with its blocks/ and references/ folders, is two folders up from this preset's folder.",
    "the kit root, with its blocks/ and references/ folders, is two folders up from this skill's folder.",
}


def sentences(text):
    # A list item or a table row starts a new sentence, so a copied bullet with no final stop is not joined to its neighbours.
    for block in re.split(r"\n\s*\n|\n(?=\s*(?:[-*+]|\d+\.|\|)\s)", text):
        for raw in re.split(r"(?<=[.!?])\s+", block):
            s = " ".join(re.sub(r"[`*_>|#-]", " ", raw.lower()).split())
            if len(s.split()) >= MIN_WORDS:
                yield s


def copies():
    found = {}
    for path in sorted(KIT.rglob("*.md")):
        if {"evals", "fixtures"} & set(path.relative_to(KIT).parts):
            continue
        for s in set(sentences(path.read_text(encoding="utf-8"))):
            found.setdefault(s, []).append(str(path.relative_to(KIT)))
    return {s: files for s, files in found.items() if len(files) > 1}


def test_no_rule_text_is_copied():
    found = copies()
    for s in sorted(ALLOWED & found.keys()):
        print(f"allowed copy: {s[:SHOW]}\n  " + "\n  ".join(found[s]))
    bad = [f"{s[:SHOW]}\n  " + "\n  ".join(files) for s, files in sorted(found.items()) if s not in ALLOWED]
    assert not bad, ("a sentence is copied between kit files; keep it in the file that owns the rule, "
                     "and make the others point to that file:\n" + "\n".join(bad))
    stale = sorted(ALLOWED - found.keys())
    assert not stale, "remove from ALLOWED, it no longer occurs twice:\n" + "\n".join(s[:SHOW] for s in stale)


if __name__ == "__main__":
    test_no_rule_text_is_copied()
    print("ok")
