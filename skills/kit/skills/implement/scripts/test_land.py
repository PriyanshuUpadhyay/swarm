"""Run: python3 test_land.py"""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import land

SCRIPT = Path(__file__).with_name("land.py")


class LandTests(unittest.TestCase):
    def test_shared_plain(self):
        self.assertEqual(land.plain.__module__, "step_run")
        self.assertEqual(land.plain("\tindent\nline\r\x1b\x85"), "\tindentline")

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.main = self.root / "main"
        self.main.mkdir()
        self.git(self.main, "init", "-q", "-b", "main")
        for key, value in (("user.name", "Test"), ("user.email", "test@example.invalid"),
                           ("commit.gpgsign", "false"), ("core.hooksPath", "/dev/null")):
            self.git(self.main, "config", key, value)
        (self.main / "shared.txt").write_text("base\n", encoding="utf-8")
        self.git(self.main, "add", "shared.txt")
        self.git(self.main, "commit", "-qm", "base")
        self.base = self.git(self.main, "rev-parse", "HEAD").strip()
        self.one, self.two = self.root / "one", self.root / "two"
        for worktree in (self.one, self.two):
            self.git(self.main, "worktree", "add", "-qb", worktree.name, str(worktree))
        self.decl = self.root / "decl.json"

    def git(self, worktree, *args):
        return subprocess.run(["git", "-C", str(worktree), *args], check=True,
                              capture_output=True, text=True).stdout

    def commit(self, worktree, file, message):
        (worktree / file).write_text(message + "\n", encoding="utf-8")
        self.git(worktree, "add", "--", file)
        self.git(worktree, "commit", "-qm", message)

    def run_land(self, declared, frozen=(), order=None, base=None):
        self.decl.write_text(json.dumps({"worktrees": {str(w): f for w, f in declared.items()},
                                         "frozen_interfaces": list(frozen)}), encoding="utf-8")
        args = [sys.executable, str(SCRIPT), str(self.decl),
                *map(str, order or (self.one, self.two))]
        if base is not None:
            args += [f"--base={base}"]
        return subprocess.run(args, cwd=self.main, capture_output=True, text=True)

    def refuse(self, result, reason):
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(reason, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_disjoint_order_and_commits(self):
        self.commit(self.one, "one.txt", "first fix")
        self.commit(self.two, "two.txt", "second fix")
        result = self.run_land({self.one: ["one.txt"], self.two: ["two.txt"]},
                               order=(self.two, self.one))
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = result.stdout.splitlines()
        self.assertEqual(lines[0], f"1. {self.two}")
        self.assertTrue(lines[1].endswith(" second fix"))
        self.assertEqual(lines[2], f"2. {self.one}")
        self.assertTrue(lines[3].endswith(" first fix"))

    def test_overlap(self):
        for worktree in (self.one, self.two):
            self.commit(worktree, "shared.txt", worktree.name)
        self.refuse(self.run_land({self.one: ["shared.txt"], self.two: ["shared.txt"]}),
                    "changed by both")

    def test_undeclared(self):
        self.commit(self.one, "extra.txt", "extra fix")
        self.refuse(self.run_land({self.one: [], self.two: []}), "outside its declared files")

    def test_frozen(self):
        self.commit(self.one, "shared.txt", "changed interface")
        self.refuse(self.run_land({self.one: ["shared.txt"], self.two: []}, ["shared.txt"]),
                    "frozen interface")

    def test_staged_and_unstaged(self):
        self.commit(self.one, "one.txt", "first fix")
        (self.one / "shared.txt").write_text("unstaged\n", encoding="utf-8")
        self.refuse(self.run_land({self.one: ["one.txt"], self.two: []}), "shared.txt")
        self.git(self.one, "add", "shared.txt")
        self.refuse(self.run_land({self.one: ["one.txt"], self.two: []}), "shared.txt")

    def test_rename_checks_old_path(self):
        self.git(self.one, "mv", "shared.txt", "renamed.txt")
        self.git(self.one, "commit", "-qm", "rename")
        self.refuse(self.run_land({self.one: ["renamed.txt"], self.two: []}), "shared.txt")

    def test_filename_with_newline(self):
        name = "line\nbreak.txt"
        self.commit(self.one, name, "odd file")
        result = self.run_land({self.one: [name], self.two: []})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_filename_with_carriage_return(self):
        name = "carriage\rreturn.txt"
        self.commit(self.one, name, "carriage return file")
        result = self.run_land({self.one: [name], self.two: []})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_terminal_controls(self):
        original = self.one
        self.one = self.root / "one\x1b[2J\x85"
        self.git(self.main, "worktree", "move", str(original), str(self.one))
        self.commit(self.one, "one.txt", "\x1b[2Jsubject\x01\x7f\x85")
        result = self.run_land({self.one: ["one.txt"], self.two: []})
        self.assertEqual(result.returncode, 0, result.stderr)
        for control in ("\x1b", "\x01", "\x7f", "\x85"):
            self.assertNotIn(control, result.stdout)
        self.assertIn("one[2J", result.stdout)
        self.assertIn("[2Jsubject", result.stdout)

    def test_worktree_path_with_newline(self):
        original = self.one
        self.one = self.root / "one\nline\rreturn"
        self.git(self.main, "worktree", "move", str(original), str(self.one))
        self.commit(self.one, "one.txt", "first fix")
        result = self.run_land({self.one: ["one.txt"], self.two: []})
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = result.stdout.splitlines()
        self.assertEqual(len(lines), 3, lines)
        self.assertEqual(lines[0], f"1. {str(self.one).replace(chr(10), '').replace(chr(13), '')}")
        self.assertEqual(lines[2], f"2. {self.two}")

    def test_closed_stdout(self):
        for number in range(300):
            self.git(self.one, "commit", "--allow-empty", "-qm", f"pipe {number} " + "x" * 500)
        self.run_land({self.one: [], self.two: []})
        with subprocess.Popen([sys.executable, SCRIPT, self.decl, self.one, self.two],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            with subprocess.Popen(["head", "-1"], stdin=child.stdout, stdout=subprocess.PIPE) as consumer:
                child.stdout.close()
                output, _ = consumer.communicate()
                self.assertEqual(consumer.returncode, 0)
                self.assertEqual(len(output.splitlines()), 1)
            errors = child.stderr.read()
            self.assertEqual(child.wait(), 0, errors)
            self.assertEqual(errors, b"")

    def test_base_override(self):
        self.commit(self.one, "one.txt", "first fix")
        result = self.run_land({self.one: [], self.two: []}, base="HEAD")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("first fix", result.stdout)
        result = self.run_land({self.one: ["one.txt"], self.two: []}, base=self.base)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("first fix", result.stdout)

    def test_merge_base_with_main_head(self):
        self.commit(self.main, "main.txt", "main moved")
        self.commit(self.one, "one.txt", "first fix")
        result = self.run_land({self.one: ["one.txt"], self.two: []})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("first fix", result.stdout)
        self.assertNotIn("main moved", result.stdout)

    def test_frozen_deletion(self):
        self.git(self.one, "rm", "shared.txt")
        self.git(self.one, "commit", "-qm", "delete interface")
        self.refuse(self.run_land({self.one: ["shared.txt"], self.two: []}, ["shared.txt"]),
                    "frozen interface")

    def test_bad_base(self):
        self.refuse(self.run_land({self.one: [], self.two: []}, base="--bad-revision"),
                    "Git failed")

    def test_bad_declaration(self):
        self.refuse(self.run_land({self.one: "one.txt", self.two: []}), "list of file paths")

    def test_missing_worktree_declaration(self):
        self.refuse(self.run_land({self.one: []}), "has no declared files")

    def test_declared_worktree_missing_from_command_line(self):
        result = self.run_land({self.one: [], self.two: []}, order=(self.one,))
        self.refuse(result, f"{self.two} is declared but missing from the command line")
        result = self.run_land({self.one: [], self.two: []}, order=(self.two,), base="HEAD")
        self.refuse(result, f"{self.one} is declared but missing from the command line")

    def test_error_path_controls(self):
        original = self.two
        self.two = self.root / "two\nline\x1b[2J"
        self.git(self.main, "worktree", "move", str(original), str(self.two))
        result = self.run_land({self.one: [], self.two: []}, order=(self.one,))
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(len(result.stderr.splitlines()), 1, result.stderr)
        self.assertNotIn("\x1b", result.stderr)
        self.assertIn("twoline[2J", result.stderr)

    def test_git_error_bytes(self):
        result = subprocess.CompletedProcess(["git"], 1, b"", b"bad \xff\x1b[2J\r\nmessage")
        with patch.object(land.subprocess, "run", return_value=result):
            with self.assertRaises(ValueError) as raised:
                land.git(self.one, "rev-parse", "HEAD")
        message = str(raised.exception)
        self.assertIn(r"bad \xff[2Jmessage", message)
        self.assertNotIn("\x1b", message)
        self.assertEqual(len(message.splitlines()), 1)


if __name__ == "__main__":
    unittest.main()
