"""Check real CLIs in a scratch home: python3 -I scripts/check-plugin-refresh.py <swarm>."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def files(folder):
    return {str(path.relative_to(folder)): path.read_bytes()
            for path in folder.rglob("*") if path.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("swarm", type=Path)
    args = parser.parse_args()
    binary = args.swarm.resolve(strict=True)
    tools = {cli: shutil.which(cli) for cli in ("claude", "codex", "agy")}
    if any(path is None for path in tools.values()):
        sys.exit("this check needs claude, codex and agy on PATH")
    repo = Path(__file__).resolve().parent.parent
    (repo / "tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="plugin-refresh-", dir=repo / "tmp") as folder:
        home = Path(folder)
        kit = home / "prefix/libexec/skills/kit"
        shutil.copytree(repo / "skills/kit", kit,
                        ignore=shutil.ignore_patterns(".git*", "__pycache__", "node_modules"))
        (home / "prefix/bin").mkdir(parents=True)
        exe = home / "prefix/bin/swarm"
        shutil.copy2(binary, exe)
        (home / "bin").mkdir()
        for cli, path in tools.items():
            (home / "bin" / cli).symlink_to(Path(path).resolve())
        (home / ".codex").mkdir()
        env = dict(HOME=str(home), SWARM_HOME=str(home), CODEX_HOME=str(home / ".codex"),
                   CLAUDE_CONFIG_DIR=str(home / ".claude"),
                   XDG_CONFIG_HOME=str(home / ".config"),
                   CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1",
                   PATH=f"{home / 'bin'}:/usr/bin:/bin")
        if "TMPDIR" in os.environ:
            env["TMPDIR"] = os.environ["TMPDIR"]

        def run(*command, success=True):
            result = subprocess.run([str(part) for part in command], env=env, cwd=home,
                                    capture_output=True, text=True, timeout=30)
            if success and result.returncode:
                raise AssertionError(f"{command}\n{result.stdout}\n{result.stderr}")
            return result

        def stamp():
            return run(sys.executable, "-I", repo / "scripts/stamp-kit.py", kit).stdout.strip()

        def stale_rows():
            doctor = json.loads(run(exe, "doctor", "--json").stdout)
            return [row for row in doctor["items"] if row["id"].startswith("plugin-")]

        def assert_copies(version):
            index = json.loads((home / ".claude/plugins/installed_plugins.json").read_text())
            install = next(row for row in index["plugins"]["swarm@swarm"]
                           if row["scope"] == "user")
            assert install["version"] == version, install
            copies = {"Claude": Path(install["installPath"]),
                      "Codex": home / f".codex/plugins/cache/swarm/swarm/{version}",
                      "AGY": home / ".gemini/config/plugins/swarm"}
            expected = files(kit)
            for cli, copy in copies.items():
                assert files(copy) == expected, f"{cli} copy differs from the kit"
            assert not stale_rows(), "fresh copies must clear the doctor rows"
            notice = run(exe, "launch", "bad/id", "chat", success=False)
            assert "stale plugin" not in notice.stderr, notice.stderr
            print(f"PASS {version}: all three copies equal the kit; doctor and launch are clear")

        first = stamp()
        run("claude", "plugin", "validate", kit, "--json")
        run("agy", "plugin", "validate", kit, "--json")
        run(exe, "setup", "plugin", "--yes")
        assert_copies(first)
        # Include an addition, a change and a removal to catch a merge that leaves old files.
        (kit / "refresh-probe.txt").write_text("new content\n")
        (kit / "README.md").write_text("changed kit README\n")
        (kit / "contracts/orchestration-requirements.schema.json").unlink()
        second = stamp()
        assert first != second
        before = files(home)
        rows = stale_rows()
        assert len(rows) == 3, rows
        notice = run(exe, "launch", "bad/id", "chat", success=False)
        assert len([line for line in notice.stderr.splitlines() if "stale plugin" in line]) == 1
        assert before == files(home), "doctor and the notice must write nothing"
        print("PASS stale upgrade: three doctor rows; one launch notice; no file changes")
        # A new package can resolve to a different Cellar folder under the same kit link.
        moved = kit.with_name("new-kit")
        kit.rename(moved)
        kit.symlink_to(moved, target_is_directory=True)
        kit = moved
        run(exe, "setup", "plugin", "--yes")
        markets = json.loads(run("codex", "plugin", "marketplace", "list", "--json").stdout)
        root = next(row["root"] for row in markets["marketplaces"] if row["name"] == "swarm")
        assert Path(root) == kit, "Codex must replace its old resolved marketplace root"
        print("PASS moved kit: Codex records the new resolved marketplace root")
        assert_copies(second)
        run(exe, "setup", "plugin", "--yes")
        assert_copies(second)


if __name__ == "__main__":
    main()
