"""Run: python3 test_preset.py. Every preset in the kit resolves, and each verb stops as the owner decided."""
import json
import subprocess
import tempfile
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


def run(*args, text=""):
    with tempfile.TemporaryDirectory() as cwd:
        out = subprocess.run([sys.executable, "-I", "-B", HERE / "preset.py", *args], input=text, capture_output=True,
                             text=True, cwd=cwd)
        assert not list(Path(cwd).iterdir()), "the script wrote a file"
    assert out.returncode == 0, out.stderr
    return json.loads(out.stdout)


def test_the_json_gives_each_step_its_stage_and_each_block_its_users():
    kit = run("--json")
    assert kit["schema"] == "swarm.kit/1" and kit["version"]
    found = presets()
    assert sorted(p["preset"] for p in kit["presets"]) == sorted(found)
    for p in kit["presets"]:
        stages = {n: i for i, names in enumerate(preset.plan_chart.stages({s["file"]: s["uses"] for s in p["steps"]}))
                  for n in names}
        for step in p["steps"]:
            assert step["stage"] == stages[step["file"]] == 1 + max((stages[u] for u in step["uses"]), default=-1)
            assert len(step["cells"]) == 5 and step["cells"][1] == step["needs"], step
            assert set(step["blocks"]) <= set(step["resolved"]), step
    blocks = {b["id"]: b for b in kit["blocks"]}
    for p in kit["presets"]:
        for step in p["steps"]:
            for block in step["resolved"]:
                assert {"preset": p["preset"], "step": step["file"], "named": block in step["blocks"]} in blocks[block]["used_by"]
    voice = blocks["voice"]
    assert voice["kind"] == "parent" and "voice-reply" in voice["parts"] and blocks["voice-reply"]["parent"] == "voice"
    assert any(not b["used_by"] for b in blocks.values()), "some block is used by no default preset"


DRAFT = """## Preset

Stop: chat

| File | Needs | Holds | Block | Why |
|---|---|---|---|---|
| 01-read | none | what the sources say | local-research, requirements | the answer rests on the sources |
| 02-web | read | what the web says | deep-search | the web adds what is not local |
| 03-reply | read, web | the answer | voice-reply | the owner reads the answer |
"""


def test_a_draft_section_or_skill_passes_with_its_stages():
    for text in (DRAFT, "---\nname: mine\ndescription: x\n---\n\n# Mine\n\n" + DRAFT):
        out = run("--check", text=text)
        assert out["ok"], out
        assert [(s["file"], s["stage"]) for s in out["preset"]["steps"]] == [("01-read", 0), ("02-web", 1), ("03-reply", 2)]
        assert out["preset"]["stop"] == "chat"


def test_a_draft_defect_is_the_first_one_python_finds():
    piped = DRAFT.replace("what the web says", "what the web | says")
    assert run("--check", text=piped) == {"ok": False, "defect": "row has 6 cells, not 5: "
                                          "| 02-web | read | what the web | says | deep-search | the web adds what is not local |"}
    assert run("--check", text=DRAFT.replace("| read, web |", "| read, nope |"))["defect"] == "step 03-reply needs 'nope', which no step gives"
    assert run("--check", text="# no preset")["defect"].endswith("has no ## Preset section")


def test_cells_are_the_rows_that_a_run_reads():
    lead = DRAFT.replace("Stop: chat\n", "Stop: chat\n\n| a | b |\n|---|---|\n| c | d |\n")
    steps = run("--check", text=lead)["preset"]["steps"]
    assert steps[0]["cells"] == ["01-read", "none", "what the sources say", "local-research, requirements",
                                 "the answer rests on the sources"], steps[0]
    twice = DRAFT.replace("|---|---|---|---|---|\n", "|---|---|---|---|---|\n| File | Needs | Holds | Block | Why |\n")
    assert [s["file"] for s in run("--check", text=twice)["preset"]["steps"]] == ["01-read", "02-web", "03-reply"]
    bare = run("--check", text=DRAFT.replace("|---|---|---|---|---|\n", ""))
    assert [s["file"] for s in bare["preset"]["steps"]] == ["01-read", "02-web", "03-reply"], bare


def test_a_new_skill_needs_a_name_and_a_description_that_yaml_reads():
    def skill(name, description):
        return f"---\nname: {name}\ndescription: {description}\n---\n\n# x\n\n" + DRAFT
    assert run("--check", text=skill("", "Use when x."))["defect"] == "name needs lowercase letters, digits, and dashes"
    assert run("--check", text=skill("my skill", "Use when x."))["defect"] == "name needs lowercase letters, digits, and dashes"
    assert run("--check", text=skill("mine", '""'))["defect"] == "description needs a `Use when` line"
    assert run("--check", text=skill("mine", "Use when: the owner asks"))["defect"] == \
        "description has YAML punctuation, so it needs double quotes"
    assert run("--check", text=skill("mine", json.dumps("Use when: the owner asks")))["ok"]
    assert run("--check", text=skill("mine", " " + json.dumps("Use when: the owner asks")))["ok"]
    assert run("--check", text=skill("mine", "'Use when: the owner''s chat asks'"))["ok"]
    assert run("--check", text=skill("mine", "'Use when: it's open'"))["defect"] == "description is not a valid quoted string"


def test_json_into_a_closed_pipe_exits_quietly():
    reader = subprocess.Popen([sys.executable, "-I", "-B", HERE / "preset.py", "--json"], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE)
    reader.stdout.close()
    assert reader.wait() == 0 and b"Traceback" not in reader.stderr.read()


def test_an_older_python_can_still_import_preset():
    old = Path("/usr/bin/python3")
    if not old.is_file():
        return
    out = subprocess.run([old, "-I", "-B", "-c", f"import sys; sys.path.insert(0, {str(HERE)!r}); import preset; print(sys.version_info[:2])"],
                         capture_output=True, text=True)
    assert out.returncode == 0, out.stderr


if __name__ == "__main__":
    test_each_preset_resolves_with_its_stop()
    test_the_command_prints_the_step_lines()
    test_the_json_gives_each_step_its_stage_and_each_block_its_users()
    test_a_draft_section_or_skill_passes_with_its_stages()
    test_a_draft_defect_is_the_first_one_python_finds()
    test_cells_are_the_rows_that_a_run_reads()
    test_a_new_skill_needs_a_name_and_a_description_that_yaml_reads()
    test_json_into_a_closed_pipe_exits_quietly()
    test_an_older_python_can_still_import_preset()
    print("ok")
