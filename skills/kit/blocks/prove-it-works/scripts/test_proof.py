"""Run the public proof commands, including their evidence guards."""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("proof.py")


class ProofTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.code = self.root / "code.py"
        self.code.write_text("value = 1\n")
        os.utime(self.code, ns=(1_000_000_000, 1_000_000_000))

    def run_proof(self, *args):
        return subprocess.run([sys.executable, "-I", str(SCRIPT), *map(str, args)], capture_output=True, text=True)

    def result(self, name, body):
        path = self.root / name
        path.write_text(body)
        os.utime(path, ns=(2_000_000_000, 2_000_000_000))
        return path

    def test_fresh_executed_tests(self):
        for name, body in (
            ("junit.xml", '<testsuite><testcase classname="Example" name="test_value"/></testsuite>'),
            ("unittest.log", "Ran 1 test in 0.001s\n\nOK\n"),
            ("pytest.log", "================= 1 passed in 0.01s =================\n"),
            ("rust.log", "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"),
            ("go.log", "--- PASS: TestValue (0.00s)\nPASS\nok example 0.001s\n"),
        ):
            with self.subTest(name=name):
                out = self.run_proof("tests", self.result(name, body), "--code", self.code)
                self.assertEqual(out.returncode, 0, out.stderr)

    def test_test_evidence_guard(self):
        for body in ("Ran 1 test in 0.001s\n\nFAILED (failures=1)\n", "1 test passed\n"):
            with self.subTest(body=body):
                out = self.run_proof("tests", self.result("guard.log", body), "--code", self.code)
                self.assertNotEqual(out.returncode, 0)
        stale = self.result("stale.log", "Ran 1 test in 0.001s\n\nOK\n")
        os.utime(self.code, ns=(3_000_000_000, 3_000_000_000))
        self.assertNotEqual(self.run_proof("tests", stale, "--code", self.code).returncode, 0)

    def test_canary_new_failure(self):
        clean = self.result("clean.xml", '<testsuite><testcase classname="Example" name="test_value"/></testsuite>')
        broken = self.result("broken.xml", '<testsuite><testcase classname="Example" name="test_value"><failure/></testcase></testsuite>')
        copy = self.root / "copy.py"
        copy.write_text("value = 0\n")
        os.utime(copy, ns=(1_000_000_000, 1_000_000_000))
        out = self.run_proof("canary", clean, broken, "--code", self.code, "--canary-code", copy)
        self.assertEqual(out.returncode, 0, out.stderr)

    def test_canary_guard(self):
        clean = self.result("clean.xml", '<testsuite><testcase name="test_value"/></testsuite>')
        broken = self.result("broken.xml", '<testsuite><testcase name="test_value"/></testsuite>')
        out = self.run_proof("canary", clean, broken, "--code", self.code, "--canary-code", self.code)
        self.assertNotEqual(out.returncode, 0)

    def test_comparison_self_check(self):
        expected = self.result("expected", "answer=1\n")
        actual = self.result("actual", "answer=1\n")
        out = self.run_proof("compare", expected, actual, "--", "cmp", "-s", "{expected}", "{actual}")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(expected.read_text(), "answer=1\n")
        self.assertEqual(actual.read_text(), "answer=1\n")

    def test_comparator_guard(self):
        expected = self.result("expected", "answer=1\n")
        actual = self.result("actual", "answer=1\n")
        for command in (["true", "{expected}", "{actual}"],
                        [sys.executable, "-c", "import sys; sys.exit(2 if open(sys.argv[1], 'rb').read() != open(sys.argv[2], 'rb').read() else 0)", "{expected}", "{actual}"]):
            with self.subTest(command=command):
                out = self.run_proof("compare", expected, actual, "--", *command)
                self.assertNotEqual(out.returncode, 0)


if __name__ == "__main__":
    unittest.main()
