"""Run: python3 test_plugin_skills.py. Check the skills in the plugin that each CLI installs."""
import json
import re
from pathlib import Path

KIT = Path(__file__).resolve().parents[1]


def test_plugin_ships_the_workflow_verbs():
    plugin = json.loads((KIT / "plugin.json").read_text(encoding="utf-8"))
    claude = json.loads((KIT / ".claude-plugin/plugin.json").read_text(encoding="utf-8"))
    marketplace = json.loads((KIT / ".claude-plugin/marketplace.json").read_text(encoding="utf-8"))
    assert plugin["name"] == "swarm"
    assert (plugin["name"], plugin["version"]) == (claude["name"], claude["version"])
    source = next(p["source"] for p in marketplace["plugins"] if p["name"] == plugin["name"])
    assert (KIT / source).resolve() == KIT
    skills = KIT / "skills"  # Each plugin uses its default skills/ folder.
    # ADR 0132: swarm is the one workflow skill, and the verbs are presets. ADR 0136: chosen standalone skills ship beside it.
    assert {p.parent.name for p in skills.glob("*/SKILL.md")} == {"swarm", "swarm-orchestrator", "swarm-voice", "skill-eval", "council"}
    presets = {p.parent.name for p in (KIT / "presets").glob("*/PRESET.md")}
    # ADR 0137: a verb that becomes a skill leaves presets/ in the same change.
    assert presets == {"build", "explain", "implement", "research", "review"}, presets


def test_background_skills_stay_available_to_the_model_and_out_of_the_menu():
    for name in ("swarm-orchestrator", "swarm-voice"):
        path = KIT / "skills" / name / "SKILL.md"
        frontmatter = path.read_text(encoding="utf-8").split("---", 2)[1]
        assert re.search(r"^user-invocable: false$", frontmatter, re.M), path
        assert not re.search(r"^disable-model-invocation: true$", frontmatter, re.M), path


def test_each_skill_name_is_its_folder_name():
    for path in sorted((KIT / "skills").glob("*/SKILL.md")):
        frontmatter = path.read_text(encoding="utf-8").split("---", 2)[1]
        name = re.search(r"^name: (.+)$", frontmatter, re.M)
        assert name and name[1] == path.parent.name, path


def test_each_description_is_short_and_says_when():
    # Owner rule 2026-10-16: at most 40 words, with "Use when", for every skill and block.
    bad = []
    for path in sorted((KIT / "skills").glob("*/SKILL.md")) + sorted((KIT / "presets").glob("*/PRESET.md")) + sorted((KIT / "blocks").glob("*/SKILL.md")):
        frontmatter = path.read_text(encoding="utf-8").split("---", 2)[1]
        text = re.search(r"^description: (.+)$", frontmatter, re.M)[1].strip().strip('"')
        if len(text.split()) > 40 or "Use when" not in text:
            bad.append(f"{path.relative_to(KIT)}: {len(text.split())} words")
    assert not bad, "\n".join(bad)


if __name__ == "__main__":
    test_plugin_ships_the_workflow_verbs()
    test_each_skill_name_is_its_folder_name()
    test_background_skills_stay_available_to_the_model_and_out_of_the_menu()
    test_each_description_is_short_and_says_when()
    print("ok")
