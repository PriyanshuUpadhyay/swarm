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
    assert {"build", "implement", "explain", "deliver"} <= {p.parent.name for p in skills.glob("*/SKILL.md")}
    assert not any((skills / name).exists() for name in ("ink", "flow"))


def test_each_skill_name_is_its_folder_name():
    for path in sorted((KIT / "skills").glob("*/SKILL.md")):
        frontmatter = path.read_text(encoding="utf-8").split("---", 2)[1]
        name = re.search(r"^name: (.+)$", frontmatter, re.M)
        assert name and name[1] == path.parent.name, path


if __name__ == "__main__":
    test_plugin_ships_the_workflow_verbs()
    test_each_skill_name_is_its_folder_name()
    print("ok")
