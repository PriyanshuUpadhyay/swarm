#!/usr/bin/env python3
"""Builds Sources/SwarmHUD/MascotSheets.swift from open-licence pixel art at pinned commits.

Run `python3 -I ui/Tools/mascots.py`; it needs `curl` and ImageMagick's `magick`. Each mascot
becomes one PNG sheet, a row per state (idle, working, waiting, sleeping) and a column per frame,
stored as base64 so the hand-built app needs no resource bundle. The open card shows each credit
below; `ui/MASCOT-NOTICE` ships them with the licence text, so a new mascot goes in both.
"""
import base64
import json
import pathlib
import subprocess
import sys
import tempfile

PETS = "https://raw.githubusercontent.com/tonybaloney/vscode-pets/93a420cab4b6618bfa9928da0dd941447a145d8f/media"
ONEKO = "https://raw.githubusercontent.com/adryd325/oneko.js/5281d057c4ea9bd4f6f997ee96ba30491aed16c0/oneko.gif"
GOPHERS = "https://raw.githubusercontent.com/egonelbre/gophers/63b1f5a9f334f9e23735c6e09ac003479ffe5df5/animation"
AGENTS = "https://raw.githubusercontent.com/pablodelucca/pixel-agents/d1e007a9fdf3003c252d2973abe1999ae59aec33/webview-ui/public/assets/characters"
# Ninja Adventure is not on pixel-boy's GitHub; this mirror carries the pack with its CC0 LICENSE.txt.
NINJA = "https://raw.githubusercontent.com/gainax2k1/godot_adventure/2011c813bd6d6f1c86ecd2ac632d175073c3d1ee/Ninja%20Adventure%20-%20Asset%20Pack/Actor/Animals"

# id, name, credit, vscode-pets colour (or None), whether the art has its own sleeping `z`.
PET_LIST = [
    ("duck", "Rubber duck", "Marc Duiker, vscode-pets (MIT)", "rubber-duck", "yellow"),
    ("crab", "Ferris", "Marc Duiker, vscode-pets (MIT); Ferris by Karen Rustad Tölva (CC0)", "crab", "red"),
    ("zappy", "Zappy", "Marc Duiker, vscode-pets (MIT)", "zappy", "yellow"),
    ("panda", "Panda", "Jessie Ferris, vscode-pets (MIT)", "panda", "black"),
    ("turtle", "Turtle", "enkeefe, vscode-pets (MIT)", "turtle", "green"),
    ("snake", "Snake", "Marc Duiker, vscode-pets (MIT)", "snake", "green"),
    ("snail", "Snail", "Kennet Shin, vscode-pets (MIT)", "snail", "brown"),
    ("chicken", "Chicken", "Gulnur Baimukhambetova, vscode-pets (MIT)", "chicken", "white"),
    ("cockatiel", "Cockatiel", "Marc Duiker, vscode-pets (MIT)", "cockatiel", "gray"),
    ("rocky", "Rocky", "Marc Duiker, vscode-pets (MIT)", "rocky", "gray"),
    ("morph", "Morph", "Luís Santos, vscode-pets (MIT)", "morph", "purple"),
    ("rat", "Rat", "szlegp, vscode-pets (MIT)", "rat", "white"),
    ("deno", "Deno", "Jo Franchetti, vscode-pets (MIT)", "deno", "green"),
]

STATES = ["idle", "working", "waiting", "sleeping"]


def sh(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def fetch(url: str, path: pathlib.Path) -> pathlib.Path:
    if not path.exists():
        subprocess.run(["curl", "-sfL", url, "-o", str(path)], check=True)
    return path


def gif_frames(gif: pathlib.Path, out: pathlib.Path) -> tuple[list[pathlib.Path], float]:
    """Every frame of a GIF as a full-canvas PNG, and its mean frame rate, at most 8 fps."""
    out.mkdir(parents=True, exist_ok=True)
    sh("magick", str(gif), "-coalesce", str(out / "f-%03d.png"))
    delays = [int(d) or 12 for d in sh("magick", "identify", "-format", "%T\n", str(gif)).split()]
    return sorted(out.glob("f-*.png")), min(round(100 * len(delays) / sum(delays), 2), 8.0)


def crop(image: pathlib.Path, geometry: str, out: pathlib.Path) -> pathlib.Path:
    sh("magick", str(image), "-crop", geometry, "+repage", str(out))
    return out


def sheet(rows: dict[str, tuple[list[pathlib.Path], float]], work: pathlib.Path) -> tuple[str, int, int, list[int], list[float]]:
    """Bottom-centres every frame on one canvas, trims to the union of all frames, and lays the
    states out as rows, so a mascot keeps its feet in place when its state changes."""
    frames = [f for state in STATES for f in rows[state][0]]
    sizes = [tuple(map(int, sh("magick", "identify", "-format", "%w %h", str(f)).split())) for f in frames]
    w, h = max(s[0] for s in sizes), max(s[1] for s in sizes)
    placed = []
    for i, f in enumerate(frames):
        p = work / f"p-{i:03d}.png"
        sh("magick", str(f), "-background", "none", "-gravity", "south", "-extent", f"{w}x{h}", str(p))
        placed.append(p)
    box = sh("magick", *map(str, placed), "-background", "none", "-layers", "merge", "-format", "%@", "info:")
    cw, ch = (int(v) for v in box.split("+")[0].split("x"))
    columns = max(len(rows[s][0]) for s in STATES)
    out = work / "sheet.png"
    sh("magick", "-size", f"{cw * columns}x{ch * len(STATES)}", "xc:none", f"PNG32:{out}")
    i = 0
    for r, state in enumerate(STATES):
        for c in range(len(rows[state][0])):
            sh("magick", str(out), "(", str(placed[i]), "-crop", box, "+repage", ")",
               "-geometry", f"+{c * cw}+{r * ch}", "-composite", f"PNG32:{out}")
            i += 1
    sh("magick", str(out), "-strip", "-define", "png:compression-level=9", f"PNG32:{out}")
    return base64.b64encode(out.read_bytes()).decode(), cw, ch, [len(rows[s][0]) for s in STATES], [rows[s][1] for s in STATES]


def main() -> None:
    root = pathlib.Path(__file__).resolve().parents[1]
    entries = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)

        # Neko: 32 px cells; (column, row) from oneko.js `spriteSets`.
        neko = fetch(ONEKO, tmp / "oneko.gif")
        cell = lambda c, r: crop(neko, f"32x32+{c * 32}+{r * 32}", tmp / f"neko-{c}-{r}.png")
        rows = {
            # Sits, then yawns every 3.5 s.
            "idle": ([cell(3, 3)] * 6 + [cell(3, 2)], 2.0),
            "working": ([cell(0, 0), cell(0, 1)], 4.0),
            "waiting": ([cell(7, 3), cell(3, 3)], 2.0),
            "sleeping": ([cell(2, 0), cell(2, 1)], 1.0),
        }
        entries.append(("neko", "Neko", "Kenji Gotoh, NekoDA 1989 (free to use); sheet from oneko.js (MIT)", True, rows))

        for mascot_id, name, credit, folder, colour in PET_LIST:
            got = {}
            for state, anim in [("idle", "idle"), ("working", "walk"), ("waiting", "swipe"), ("sleeping", "lie")]:
                gif = tmp / f"{folder}-{anim}.gif"
                try:
                    fetch(f"{PETS}/{folder}/{colour}_{anim}_8fps.gif", gif)
                except subprocess.CalledProcessError:
                    continue
                got[state] = gif_frames(gif, tmp / f"{folder}-{anim}")
            # No lie-down art: the first idle frame, and the HUD draws its own `z`.
            got.setdefault("sleeping", ([got["idle"][0][0]], 1.0))
            entries.append((mascot_id, name, credit, False, got))

        gopher_credit = "Egon Elbre (CC0); Go gopher by Renee French (CC BY 4.0)"
        looking = gif_frames(fetch(f"{GOPHERS}/looking-around.gif", tmp / "g-look.gif"), tmp / "g-look")
        run = gif_frames(fetch(f"{GOPHERS}/2bit-sprite/run.gif", tmp / "g-run.gif"), tmp / "g-run")
        walk = gif_frames(fetch(f"{GOPHERS}/2bit-sprite/walk.gif", tmp / "g-walk.gif"), tmp / "g-walk")
        entries.append(("gopher", "Go gopher", gopher_credit, False,
                        {"idle": looking, "working": run, "waiting": walk, "sleeping": ([looking[0][0]], 1.0)}))

        # Pixel Agents: 16 x 32 cells, row 0 faces front; 0-2 walk, 3-4 type, 5-6 read.
        for n in range(6):
            agent = fetch(f"{AGENTS}/char_{n}.png", tmp / f"agent{n}.png")
            pc = lambda c: crop(agent, f"16x32+{c * 16}+0", tmp / f"agent{n}-{c}.png")
            entries.append(("worker" if n == 0 else f"worker{n + 1}", "Office worker" if n == 0 else f"Office worker {n + 1}",
                            "pixel-agents (MIT); art from JIK-A-4 MetroCity (CC0)", False, {
                "idle": ([pc(1)], 1.0), "working": ([pc(3), pc(4)], 4.0),
                "waiting": ([pc(5), pc(6)], 2.0), "sleeping": ([pc(1)], 1.0),
            }))

        # Ninja Adventure animals: two 16 px frames, sitting and leaping.
        for mascot_id, name in [("frog", "Frog"), ("raccoon", "Raccoon")]:
            actor = fetch(f"{NINJA}/{name.replace('Raccoon', 'Racoon')}/SpriteSheet.png", tmp / f"{mascot_id}.png")
            sit, leap = (crop(actor, f"16x16+{c * 16}+0", tmp / f"{mascot_id}-{c}.png") for c in (0, 1))
            entries.append((mascot_id, name, "Pixel-boy and AAA, Ninja Adventure (CC0)", False, {
                "idle": ([sit], 1.0), "working": ([sit, leap], 4.0),
                "waiting": ([leap, sit], 2.0), "sleeping": ([sit], 1.0),
            }))

        lines = [
            "// Generated by ui/Tools/mascots.py; do not edit. Licences: ui/MASCOT-NOTICE.",
            "",
            "extension SheetMascot {",
            "    static let all: [SheetMascot] = [",
        ]
        for mascot_id, name, credit, own_z, rows in entries:
            work = tmp / f"sheet-{mascot_id}"
            work.mkdir()
            png, cw, ch, counts, fps = sheet(rows, work)
            lines += [
                f"        SheetMascot(id: {json.dumps(mascot_id)}, name: {json.dumps(name)}, credit: {json.dumps(credit, ensure_ascii=False)}, ownZ: {str(own_z).lower()},",
                f"                    cell: CGSize(width: {cw}, height: {ch}), frames: {counts}, fps: {fps},",
                f'                    png: "{png}"),',
            ]
        lines += ["    ]", "}", ""]
    target = root / "Sources/SwarmHUD/MascotSheets.swift"
    target.write_text("import CoreGraphics\n\n" + "\n".join(lines))
    print(target, f"{target.stat().st_size // 1024} KiB", file=sys.stderr)


if __name__ == "__main__":
    main()
