"""Test-gate proof goes through build and verdict, with a real unittest runner."""
import shlex
from test_review_check_support import *


def test_gate_repo(command, marker=""):
    base = commit({"value.py": "value = 1\n"}, "base")
    head = commit({"value.py": "value = 2\n",
                   "test_value.py": "import unittest\nfrom value import value\nclass ValueTests(unittest.TestCase):\n"
                                    "    def test_value(self):\n        self.assertEqual(value, 2)\n",
                   "check.sh": f"{shlex.quote(sys.executable)} -m unittest discover\n"}, "change")
    rules = Path("gate.md")
    rules.write_text(f"CI: `Tests` runs `{command}`{marker}\n")
    ci = rc.ci_of(rules)
    with patch.object(rc, "ci_of", return_value=ci):
        rc.start(f"{base}..{head}")
    run = Path("tmp/review-check") / f"range-{base[:7]}-{head[:7]}-01"
    checklist = load_checklist(run)
    answers = []
    for unit in json.loads((run / "units.json").read_text()):
        line = unit["lines"][0]
        quote = Path(unit["file"]).read_text().splitlines()[line - 1]
        rules = ', '.join(checklist[unit['id']]['rules']) or '-'
        answers.append(f"| {unit['id']} | {rules} | {unit['file']}:{line} | `{quote}` | n/a | | "
                       "this fixture has no case for the listed rules |")
    rows(run, *answers)
    return run


def test_parsed_test_build_gate():
    with scratch_repo():
        run = test_gate_repo("sh check.sh", "; tests")
        rc.build(run.name, "--run")
        result = json.loads((run / "build.json").read_text())
        assert result["conclusion"] == "success", result
        assert (run / "build-tests.log").is_file()
        assert rc.verdict(run.name) == 0
        assert "Verdict: APPROVE" in (run / "03-verdict.md").read_text()


def test_test_gate_proof_guard():
    with scratch_repo():
        run = test_gate_repo(f"{shlex.quote(sys.executable)} -m unittest discover -p no_tests.py || true")
        rc.build(run.name, "--run")
        result = json.loads((run / "build.json").read_text())
        assert result["conclusion"] == "incomplete", result
        assert rc.verdict(run.name) == 1
        assert "Verdict: INCOMPLETE" in (run / "03-verdict.md").read_text()
        target = json.loads((run / "target.json").read_text())
        target["ci"]["cmd"] = f"{shlex.quote(sys.executable)} -m unittest discover"
        (run / "target.json").write_text(json.dumps(target))
        rc.build(run.name, "--run")
        assert json.loads((run / "build.json").read_text())["conclusion"] == "success"
        (run / "build-tests.log").unlink()
        assert rc.verdict(run.name) == 1
        assert "Verdict: INCOMPLETE" in (run / "03-verdict.md").read_text()


if __name__ == "__main__":
    test_parsed_test_build_gate()
    test_test_gate_proof_guard()
    print("ok")
