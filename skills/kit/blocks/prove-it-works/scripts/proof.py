#!/usr/bin/env python3
"""Check kept test results, canary failures, and a comparator's one-byte self-check.

This file owns the mechanical proof guards. It does not prove test coverage or
that a chosen mutation, mask, tolerance, or set of source paths is sufficient.
"""
import argparse
import json
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path


def parse_result(path):
    """Read JUnit testcase rows or unittest, pytest, Rust, and verbose Go logs."""
    text = Path(path).read_text(encoding="utf-8")
    if text.lstrip().startswith("<"):
        try:
            root = ET.fromstring(text)
        except ET.ParseError as error:
            raise ValueError(f"unreadable JUnit result: {error}") from None
        cases = [e for e in root.iter() if e.tag.rsplit("}", 1)[-1] == "testcase"]
        failures, executed = set(), 0
        for case in cases:
            states = {child.tag.rsplit("}", 1)[-1] for child in case}
            if "skipped" in states:
                continue
            executed += 1
            if states & {"failure", "error"}:
                failures.add((case.get("classname", ""), case.get("name", "")))
        return {"executed": executed, "failed": len(failures), "failures": sorted(failures), "format": "junit"}
    # Parse every completed suite, rather than a hand-entered total or just the last green line.
    suites = list(re.finditer(r"^Ran (\d+) tests? in [^\n]+\n\s*\n?(OK(?: \([^\n]*\))?|FAILED \([^\n]*\)|NO TESTS RAN)$", text, re.M))
    if suites:
        executed = failed = 0
        for suite in suites:
            status = suite[2]
            counts = dict((key, int(n)) for key, n in re.findall(r"(\w+)=(\d+)", status))
            executed += int(suite[1]) - counts.get("skipped", 0)
            failed += max(int(status.startswith("FAILED")), counts.get("failures", 0) + counts.get("errors", 0))
        return {"executed": executed, "failed": failed, "format": "unittest"}
    suites = re.findall(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text, re.M)
    if suites:
        return {"executed": sum(int(p) + int(f) for p, f, _ in suites),
                "failed": sum(int(f) for _, f, _ in suites), "format": "rust"}
    suites = re.findall(r"^(?:=+ )?((?:\d+ (?:passed|failed|skipped|deselected|xfailed|xpassed|errors?)(?:, )?)+) in [^\n]+$", text, re.M)
    if suites:
        counts = [(int(n), state) for suite in suites for n, state in re.findall(r"(\d+) (\w+)", suite)]
        return {"executed": sum(n for n, s in counts if s in ("passed", "failed", "xpassed", "xfailed", "error", "errors")),
                "failed": sum(n for n, s in counts if s in ("failed", "error", "errors")), "format": "pytest"}
    cases = re.findall(r"^--- (PASS|FAIL|SKIP): [^\n]+$", text, re.M)
    if cases or "[no test files]" in text or "[no tests to run]" in text:
        return {"executed": sum(state != "SKIP" for state in cases), "failed": cases.count("FAIL"), "format": "go"}
    raise ValueError("no supported runner result found; keep JUnit testcase rows or a full runner log")


def test_result(result, code, allow_failures=False):
    result = Path(result)
    if not code:
        raise ValueError("name the source and test paths with --code")
    sources = []
    for value in code:
        path = Path(value)
        if path.is_file():
            sources.append(path)
        elif path.is_dir():
            sources.extend(p for p in path.rglob("*") if p.is_file())
        else:
            raise ValueError(f"code path does not exist: {path}")
    if not sources:
        raise ValueError("code paths contain no files")
    return parsed_tests(result, max(p.stat().st_mtime_ns for p in sources), allow_failures)


def parsed_tests(result, code_mtime_ns, allow_failures=False):
    if Path(result).stat().st_mtime_ns <= code_mtime_ns:
        raise ValueError("test result is not newer than the code; run the tests again")
    counts = parse_result(result)
    if counts["executed"] <= 0:
        raise ValueError("zero tests executed; a green exit alone is not test proof")
    if counts["failed"] and not allow_failures:
        raise ValueError("the kept test run has failures")
    return {**counts, "code_mtime_ns": code_mtime_ns}


def canary(clean, broken, code, canary_code):
    baseline = test_result(clean, code)
    changed = test_result(broken, canary_code, allow_failures=True)
    if baseline["format"] != "junit" or changed["format"] != "junit":
        raise ValueError("canary needs JUnit testcase rows so new failing test IDs can be compared")
    new = sorted(set(map(tuple, changed["failures"])) - set(map(tuple, baseline["failures"])))
    if not new:
        raise ValueError("canary made no new test fail; choose a break that the tests detect")
    return {"new_failures": new, "limit": "some test can fail; this does not prove coverage"}


def compare(expected, actual, command, byte=0):
    if not command or not any("{expected}" in word for word in command) or not any("{actual}" in word for word in command):
        raise ValueError("comparator command needs {expected} and {actual} path arguments")
    expected, actual = Path(expected).resolve(), Path(actual).resolve()
    original = expected.read_bytes()
    actual.read_bytes()
    if not 0 <= byte < len(original):
        raise ValueError("choose an existing expected byte with --byte; an empty expected file cannot self-check")

    def run(path):
        args = [word.replace("{expected}", str(path)).replace("{actual}", str(actual)) for word in command]
        return subprocess.run(args, capture_output=True)

    baseline = run(expected)
    if baseline.returncode != 0:
        raise ValueError(f"comparison did not match (exit {baseline.returncode})")
    # Keep the original golden intact. Use the same comparator options on the changed copy.
    with tempfile.TemporaryDirectory(prefix="proof-compare-") as folder:
        changed = Path(folder) / expected.name
        changed.write_bytes(original[:byte] + bytes([original[byte] ^ 1]) + original[byte + 1:])
        checked = run(changed)
    if checked.returncode != 1:
        raise ValueError(f"one-byte self-check needs difference exit 1, got {checked.returncode}; exit 2 or more is an error")
    return {"comparison": "same", "self_check": "difference", "byte": byte,
            "limit": "one detected byte does not prove masks or tolerances preserve every real difference"}


def runs_tests(command):
    """Recognize direct test commands; opaque wrappers must be named as test gates."""
    return bool(re.search(r"(?:^|[\s/;&|])(?:pytest|unittest|jest|vitest)(?:\s|$)|"
                          r"\b(?:go|cargo|npm|pnpm|yarn|bun|swift|dotnet)\s+(?:run\s+)?test\b|"
                          r"\bnode\s+--test\b|\bpython[\d.]*\s+[^;&|]*\btest_[\w./-]+\.py\b", command))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="kind", required=True)
    tests = sub.add_parser("tests", help="check a parsed test result against the tested code",
                           description="Read JUnit testcase rows or full unittest, pytest, Rust, or verbose Go logs. "
                           "Require executed tests, no failures, and a result newer than every named source/test file.")
    tests.add_argument("result")
    tests.add_argument("--code", nargs="+", required=True)
    mutation = sub.add_parser("canary", help="compare clean and broken JUnit test IDs")
    mutation.add_argument("clean")
    mutation.add_argument("broken")
    mutation.add_argument("--code", nargs="+", required=True)
    mutation.add_argument("--canary-code", nargs="+", required=True)
    comparison = sub.add_parser("compare", help="run a comparator, then its one-byte self-check",
                                description="Use the same comparator for the original and the changed expected copy. "
                                "The comparator must exit 0 for same, 1 for different, and 2 or more for an error. "
                                "Use --byte before the file paths to select a meaningful unmasked byte.")
    comparison.add_argument("expected")
    comparison.add_argument("actual")
    comparison.add_argument("--byte", type=int, default=0)
    comparison.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        if args.kind == "tests":
            result = test_result(args.result, args.code)
        elif args.kind == "canary":
            result = canary(args.clean, args.broken, args.code, args.canary_code)
        else:
            command = args.command[1:] if args.command[:1] == ["--"] else args.command
            result = compare(args.expected, args.actual, command, args.byte)
    except (OSError, ValueError, ET.ParseError) as error:
        parser.exit(1, f"proof: {error}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
