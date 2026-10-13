"""Stdlib regression tests for validate_citations (no pytest dependency).

Run: python3 test_validate_citations.py

Focus: match_file path resolution — the basename-collision bug that flagged 6 correct
citations as failures on a PR with three same-named `index.ts` files.
"""
import sys
import traceback

from validate_citations import match_file, parse_diff, overlaps, looks_like_path

THREE_INDEX = [
    "apps/control-plane/src/index.ts",
    "apps/dataplane/src/index.ts",
    "apps/speaker-identification/src/index.ts",
]


# --- match_file: the regression ------------------------------------------------
def test_path_qualified_resolves_to_its_own_file_not_first_basename():
    # control-plane is first in the list; a path-qualified dataplane/speaker-id citation
    # must NOT collapse to it (the original bug).
    assert match_file("apps/dataplane/src/index.ts", THREE_INDEX) == "apps/dataplane/src/index.ts"
    assert match_file("apps/speaker-identification/src/index.ts", THREE_INDEX) == \
        "apps/speaker-identification/src/index.ts"
    assert match_file("apps/control-plane/src/index.ts", THREE_INDEX) == \
        "apps/control-plane/src/index.ts"


def test_bare_ambiguous_basename_returns_none():
    # "index.ts" matches all three -> ambiguous -> force a path-qualified citation.
    assert match_file("index.ts", THREE_INDEX) is None


def test_bare_unique_basename_resolves():
    files = ["src/pay.py", "src/util/strings.py"]
    assert match_file("pay.py", files) == "src/pay.py"


def test_partial_unique_path_suffix_resolves():
    assert match_file("control-plane/src/index.ts", THREE_INDEX) == \
        "apps/control-plane/src/index.ts"


def test_exact_match_wins():
    assert match_file("apps/dataplane/src/index.ts", THREE_INDEX) == "apps/dataplane/src/index.ts"


# --- supporting helpers --------------------------------------------------------
def test_parse_diff_new_side_ranges():
    import tempfile, os
    diff = (
        "diff --git a/x.ts b/x.ts\n--- a/x.ts\n+++ b/x.ts\n"
        "@@ -10,2 +12,3 @@\n+a\n+b\n"
    )
    fd, p = tempfile.mkstemp(suffix=".patch")
    try:
        with os.fdopen(fd, "w") as fh:
            fh.write(diff)
        files = parse_diff(p)  # parse_diff takes a FILE PATH
        assert "x.ts" in files
        assert (12, 14) in files["x.ts"]
    finally:
        os.unlink(p)


def test_parse_diff_strips_mnemonic_prefixes():
    # `git diff` with diff.mnemonicPrefix=true emits i/ w/ (not a/ b/); the path must
    # still normalize to x.ts so citations resolve against it.
    import tempfile, os
    diff = (
        "diff --git i/x.ts w/x.ts\n--- i/x.ts\n+++ w/x.ts\n"
        "@@ -10,2 +12,3 @@\n+a\n+b\n"
    )
    fd, p = tempfile.mkstemp(suffix=".patch")
    try:
        with os.fdopen(fd, "w") as fh:
            fh.write(diff)
        files = parse_diff(p)
        assert "x.ts" in files
        assert (12, 14) in files["x.ts"]
    finally:
        os.unlink(p)


def test_failure_names_the_checked_file_and_line():
    import os, subprocess, tempfile
    with tempfile.TemporaryDirectory() as d:
        diff = os.path.join(d, "diff.patch")
        checked = os.path.join(d, "map.md")
        with open(diff, "w") as fh:
            fh.write("diff --git a/x.ts b/x.ts\n--- a/x.ts\n+++ b/x.ts\n@@ -10,2 +12,3 @@\n+a\n+b\n")
        with open(checked, "w") as fh:
            fh.write("# map\n| behavior | x.ts:40 |\n")
        script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "validate_citations.py")
        out = subprocess.run([sys.executable, script, diff, checked], capture_output=True, text=True)
        assert out.returncode == 1
        assert f"({checked}:2)" in out.stdout
        assert "explanation.md" not in out.stdout

def test_overlaps_range_intersection():
    assert overlaps(4420, 4420, [(4417, 4467)]) is True
    assert overlaps(4442, 4442, [(4417, 4467)]) is True
    assert overlaps(5000, 5000, [(4417, 4467)]) is False


def test_looks_like_path_filters_noise():
    assert looks_like_path("src/index.ts") is True
    assert looks_like_path("12") is False  # not a path (would be "12:30"-style noise)


# ------------------------------------------------------------------------------
def _run():
    g = globals()
    tests = sorted(n for n in g if n.startswith("test_") and callable(g[n]))
    failed = 0
    for n in tests:
        try:
            g[n]()
            print(f"PASS {n}")
        except Exception:  # noqa: BLE001
            failed += 1
            print(f"FAIL {n}")
            traceback.print_exc()
    print(f"\n{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(_run())
