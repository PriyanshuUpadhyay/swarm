# Implement test archive cases and helpers.
from test_implement_run_support import *


def test_catalog_uses_running_kit(tmp):
    local = implement_run.BLOCKS.parent / "skills" / "engineering-standards" / "SKILL.md"
    installed = "~/.swarm/kit/skills/engineering-standards/SKILL.md"
    absolute = str(Path(tmp, "running-kit", "home", ".swarm", "kit", "skills", "engineering-standards", "SKILL.md"))
    missing = "~/.swarm/kit/skills/not-shipped/SKILL.md"
    outside = "~/.swarm/kit-other/skills/example/SKILL.md"
    out, folder, _, _ = blocks_start(tmp, "running-kit", f"- build: {installed}, {absolute}, {missing}, {outside}\n")
    assert out.returncode == 0, out.stderr
    text = step_text(folder, "05-build")
    skills = next(line for line in text.splitlines() if line.startswith("Skills read:"))
    assert skills.count(str(local)) == 1 and installed not in skills and absolute not in skills, skills
    assert missing in skills and outside in skills, skills
    assert f"===== {local} =====" in text


def test_close_from_other_cwd(tmp):
    out, folder, repo, run = blocks_start(tmp, "archive-other-cwd", "", None)
    assert out.returncode == 0, out.stderr
    fill(folder / "07-close.md")
    archived = folder.resolve().parent / "_closed" / folder.name
    outside = Path(tmp) / "outside-repo"
    outside.mkdir()
    env = {**git_env(), "GIT_CEILING_DIRECTORIES": str(outside.parent)}
    out = subprocess.run([sys.executable, SCRIPT, "done", str(folder), "07-close"], cwd=outside,
                         env=env, capture_output=True, text=True)
    assert out.returncode == 0 and out.stdout.splitlines()[-1] == str(archived), out
    assert not folder.exists() and (archived / "07-close.md").read_text(encoding="utf-8").startswith("Status: done ")
    assert not (outside / "tmp").exists()
    assert run("status", archived).returncode == 0
    again = run("done", archived, "07-close")
    assert again.returncode == 1 and "already closed" in again.stderr, again
    assert not (archived.parent / "_closed").exists()


def test_second_same_day_close(tmp):
    out, folder, repo, run = blocks_start(tmp, "archive-second-run", "", None)
    assert out.returncode == 0, out.stderr
    fill(folder / "07-close.md")
    assert run("done", folder, "07-close").returncode == 0
    archived = folder.parent / "_closed" / folder.name
    first = (archived / "07-close.md").read_text(encoding="utf-8")
    out = run("start")
    assert out.returncode == 0 and Path(out.stdout.splitlines()[0]) == folder, out
    fill(folder / "07-close.md")
    out = run("done", folder, "07-close")
    assert out.returncode == 1 and "rename the run folder" in out.stderr, out
    assert (folder / "07-close.md").read_text(encoding="utf-8").startswith("Status: open\n")
    renamed = folder.with_name(folder.name + "-second")
    folder.rename(renamed)
    out = run("done", renamed, "07-close")
    second = archived.with_name(renamed.name)
    assert out.returncode == 0 and out.stdout.splitlines()[-1] == str(second), out
    assert (second / "07-close.md").read_text(encoding="utf-8").startswith("Status: done ")
    assert (archived / "07-close.md").read_text(encoding="utf-8") == first


def test_close_archives_run(tmp):
    out, folder, repo, run = blocks_start(tmp, "archive-close", "", None)
    assert out.returncode == 0, out.stderr
    close = folder / "07-close.md"
    archived = repo.resolve() / "tmp" / "flow" / "_closed" / folder.name
    before = close.read_text(encoding="utf-8")
    out = run("done", folder, "07-close")
    assert out.returncode == 1 and close.read_text(encoding="utf-8") == before
    assert not archived.exists()
    fill(close)
    archived.mkdir(parents=True)
    (archived / "keep.md").write_text("older run", encoding="utf-8")
    out = run("done", folder, "07-close")
    assert out.returncode == 1 and "already exists" in out.stderr, out
    assert close.read_text(encoding="utf-8").startswith("Status: open\n")
    assert (archived / "keep.md").read_text(encoding="utf-8") == "older run"
    (archived / "keep.md").unlink()
    archived.rmdir()
    out = run("done", folder, "07-close")
    assert out.returncode == 0 and out.stdout.splitlines()[-1] == str(archived), out
    assert not folder.exists() and (archived / "07-close.md").read_text(encoding="utf-8").startswith("Status: done ")
    assert "\t07-close\tdone " in (archived / "events.log").read_text(encoding="utf-8")
    assert "no implement run folder" in run("status").stderr
    assert run("status", archived).returncode == 0
