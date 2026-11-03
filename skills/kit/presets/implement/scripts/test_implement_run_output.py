# Implement test output cases and helpers.
from test_implement_run_support import *


def test_block_ascii_locale(tmp):
    """start reads .git/info/exclude, domains.md, profile.md, the block text (`…` in web-ui-basics), and catalog skill text as
    UTF-8 under an ASCII locale."""
    assert_ascii_locale()
    skill = Path(tmp, "ascii-skill.md")
    skill.write_text("# skill\nRULE § 9\n", encoding="utf-8")
    out, folder, _, run = blocks_start(tmp, "ascii", f"# é note\n- build: block:web-ui-basics, {skill}\n", env=ASCII_ENV)
    assert out.returncode == 0, out.stderr
    assert "…" in step_text(folder, "05-build") and "RULE § 9" in step_text(folder, "05-build")
    Path(tmp, "ascii", "home", ".flow", "app", "profile.md").write_text("# é note\nDomains: any\nRepo rules: none\n",
                                                                       encoding="utf-8")
    # A non-ASCII line in .git/info/exclude and no /tmp/ line yet: start reads it as UTF-8 and appends /tmp/.
    exclude = Path(tmp, "ascii", "app", ".git", "info", "exclude")
    kept = [line for line in exclude.read_text(encoding="utf-8").splitlines() if line != "/tmp/"]
    exclude.write_text("\n".join(["# café note", *kept]) + "\n", encoding="utf-8")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0 and "RULE § 9" in step_text(folder, "05-build"), out.stderr
    assert "could not add /tmp/" not in out.stderr, out.stderr
    lines = exclude.read_text(encoding="utf-8").splitlines()
    assert lines[0] == "# café note" and lines[-1] == "/tmp/", lines
    # An exclude file that is not UTF-8 gets a one-line note, and start goes on.
    exclude.write_bytes(b"\xff\n")
    shutil.rmtree(folder)
    out = run("start")
    assert out.returncode == 0 and "could not add /tmp/" in out.stderr and "Traceback" not in out.stderr, out.stderr
