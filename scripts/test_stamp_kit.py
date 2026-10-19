"""Run with python3 -I scripts/test_stamp_kit.py."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("stamp-kit.py")


class StampKitTest(unittest.TestCase):
    def test_content_changes_version_and_both_manifests_match(self):
        with tempfile.TemporaryDirectory() as folder:
            kit = Path(folder)
            (kit / ".claude-plugin").mkdir()
            for name in ("plugin.json", ".claude-plugin/plugin.json"):
                (kit / name).write_text(json.dumps({"name": "swarm", "version": "0.1.0"}))
            content = kit / "SKILL.md"
            content.write_text("first kit")

            def stamp():
                result = subprocess.run([sys.executable, "-I", str(SCRIPT), str(kit)],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                a = json.loads((kit / "plugin.json").read_text())["version"]
                b = json.loads((kit / ".claude-plugin/plugin.json").read_text())["version"]
                self.assertEqual(a, b)
                self.assertRegex(a, r"^0\.1\.0\+[0-9a-f]{16}$")
                return a

            first = stamp()
            self.assertEqual(stamp(), first, "stamping twice must be stable")
            content.touch()
            self.assertEqual(stamp(), first, "mtime must not change the version")
            content.chmod(0o755)
            self.assertNotEqual(stamp(), first, "executable bits are kit content")
            content.chmod(0o644)
            self.assertEqual(stamp(), first)
            content.write_text("second kit")
            second = stamp()
            self.assertNotEqual(second, first)
            content.rename(kit / "RENAMED.md")
            self.assertNotEqual(stamp(), second, "file names are kit content")
            (kit / "RENAMED.md").unlink()
            self.assertNotEqual(stamp(), first, "removed files must change the version")

    def test_release_and_local_install_use_one_packaging_step(self):
        repo = SCRIPT.parent.parent
        build = (repo / "ui/Tools/build.sh").read_text()
        self.assertEqual(build.count('"$repo/scripts/stamp-kit.py"'), 1)
        self.assertIn('make -C ui package', (repo / ".github/workflows/release.yml").read_text())
        self.assertIn('make -C "$work/tree/ui" package',
                      (repo / "scripts/install-local.sh").read_text())
        self.assertIn('libexec.install Dir["libexec/*"]',
                      (repo / ".github/workflows/tap.yml").read_text())


if __name__ == "__main__":
    unittest.main()
