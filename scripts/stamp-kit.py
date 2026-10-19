"""Stamp the staged release kit. Source checkouts keep their manifest versions."""
import hashlib
import json
from pathlib import Path
import sys


def stamp(kit: Path) -> str:
    manifests = [kit / "plugin.json", kit / ".claude-plugin/plugin.json"]
    values = [json.loads(path.read_text(encoding="utf-8")) for path in manifests]
    bases = [value["version"].split("+", 1)[0] for value in values]
    if bases[0] != bases[1]:
        raise ValueError("the kit manifests must have the same base version")
    # Hash the unstamped manifests so a repeated stamp has the same result.
    digest = hashlib.sha256()
    for path in sorted(kit.rglob("*")):
        if not path.is_file():
            continue
        data = path.read_bytes()
        if path in manifests:
            value = dict(values[manifests.index(path)], version=bases[0])
            data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
        name = path.relative_to(kit).as_posix().encode()
        mode = (path.stat().st_mode & 0o111).to_bytes(2, "big")
        for part in (name, mode, data):
            digest.update(len(part).to_bytes(8, "big"))
            digest.update(part)
    version = f"{bases[0]}+{digest.hexdigest()[:16]}"
    for path, value in zip(manifests, values):
        value["version"] = version
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    return version


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: python3 -I scripts/stamp-kit.py <staged-kit>")
    print(stamp(Path(sys.argv[1])))
