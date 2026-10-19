"""Run: python3 test_preset.py. Every preset in the kit resolves, and each verb stops as the owner decided."""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
KIT = HERE.parents[2]
sys.path.insert(0, str(HERE))
import preset  # noqa: E402

# Owner decision 2026-10-16: these verbs wait for the Plannotator click; explain and council go on in chat.
STOPS = {"implement": "owner", "build": "owner", "research": "owner", "review check": "owner",
         "review walk": "owner", "review cleanup": "owner", "explain": "chat", "council": "chat"}


def presets():
    """{"<verb>[ <mode>]": file} for each file with a ## Preset section."""
    found = {}
    for file in sorted((KIT / "skills").glob("*/SKILL.md")) + sorted((KIT / "skills").glob("*/references/*.md")):
        if "\n## Preset\n" in file.read_text(encoding="utf-8"):
            verb = file.parent.name if file.name == "SKILL.md" else f"{file.parents[1].name} {file.stem}"
            found[verb] = file
    return found


def test_each_preset_resolves_with_its_stop():
    found = presets()
    assert set(found) == set(STOPS), sorted(set(found) ^ set(STOPS))
    for name, file in found.items():
        stop, _, steps = preset.check(file)
        assert STOPS.get(name) == stop, (name, stop)
        assert steps and all(blocks for _, _, blocks in steps), name


def test_the_command_prints_the_step_lines():
    name = next(iter(presets()), None)
    if name is None:
        return
    out = subprocess.run([sys.executable, "-I", HERE / "preset.py", *name.split()], capture_output=True, text=True)
    assert out.returncode == 0 and out.stdout.startswith("Stop: ") and " | needs " in out.stdout, out
    out = subprocess.run([sys.executable, "-I", HERE / "preset.py", "no-such-verb"], capture_output=True, text=True)
    assert out.returncode == 1 and "no preset file" in out.stderr, out


if __name__ == "__main__":
    test_each_preset_resolves_with_its_stop()
    test_the_command_prints_the_step_lines()
    print("ok")
