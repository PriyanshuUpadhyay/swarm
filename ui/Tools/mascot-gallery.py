#!/usr/bin/env python3
"""Writes a one-file HTML gallery of every HUD mascot, each in its four states, animated.

Run `python3 -I ui/Tools/mascot-gallery.py [out.html]`; the default is `ui/.build/mascots.html`.
It reads the own art from `MascotArt.swift` and the guests from `MascotSheets.swift`, so the page
shows what the build ships. The motion copies `mascotPose` and `mascotCue` in `Mascot.swift`.
Codex pets are the owner's own files and stay out.
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCES = ROOT / "Sources/SwarmHUD"
KEYS = ["idle", "blink", "working", "waiting", "sleeping"]


def own_art() -> list[dict]:
    text = (SOURCES / "MascotArt.swift").read_text()
    blocks = re.split(r"static let (\w+) = MascotArt\(", text)[1:]
    out = []
    for name, body in zip(blocks[::2], blocks[1::2]):
        palette = {k: f"#{v}" for k, v in re.findall(r'"(\w)": Color\(hex: 0x([0-9A-Fa-f]{6})\)', body)}
        art = {"id": name, "palette": palette}
        for i, key in enumerate(KEYS):
            end = body.find(f"{KEYS[i + 1]}:") if i + 1 < len(KEYS) else len(body)
            rows = re.findall(r'"([.A-Za-z]{12})"', body[body.find(f"{key}:"):end])
            frames = [rows[j:j + 10] for j in range(0, len(rows), 10)]
            art[key] = frames if key == "working" else frames[0]
        out.append(art)
    return out


def guests() -> list[dict]:
    text = (SOURCES / "MascotSheets.swift").read_text()
    pattern = (r'SheetMascot\(name: (".*?"), credit: (".*?"), ownZ: (\w+),\s*'
               r'cell: CGSize\(width: (\d+), height: (\d+)\), frames: (\[.*?\]), fps: (\[.*?\]),\s*png: "(.*?)"\)')
    return [{"name": json.loads(n), "credit": json.loads(c), "ownZ": z == "true", "w": int(w), "h": int(h),
             "frames": json.loads(f), "fps": json.loads(p), "png": png}
            for n, c, z, w, h, f, p, png in re.findall(pattern, text)]


PAGE = """<!doctype html>
<meta charset="utf-8">
<title>Swarm HUD mascots</title>
<style>
  body { margin: 0; padding: 24px; background: #111114; color: #e8e8ec; font: 14px -apple-system, system-ui, sans-serif; }
  h1 { font-size: 20px; margin: 0 0 4px; }
  p.note { color: #8f8f94; margin: 0 0 20px; }
  label { color: #a8a8b0; margin-right: 16px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(420px, 1fr)); gap: 14px; margin-top: 16px; }
  .card { background: #1c1c21; border-radius: 12px; padding: 12px 14px; }
  .card h2 { font-size: 15px; margin: 0; }
  .card small { color: #8f8f94; display: block; margin: 2px 0 10px; min-height: 2.4em; }
  .states { display: flex; gap: 8px; }
  .state { flex: 1; text-align: center; color: #8f8f94; font-size: 11px; }
  canvas { display: block; width: 100%; height: 72px; background: #000; border-radius: 14px; margin-bottom: 4px; }
</style>
<h1>Swarm HUD mascots</h1>
<p class="note">COUNT mascots in the notch cast, drawn from the shipped sources. Turn 0 is the busiest provider's own art, then the bee, then the guests.</p>
<label><input type="checkbox" id="strained"> Strained (sweat drop)</label>
<label><input type="checkbox" id="still"> Still frames (Reduce Motion)</label>
<div class="grid" id="grid"></div>
<script>
const OWN = OWN_JSON, GUESTS = GUESTS_JSON;
const STATES = ["idle", "working", "waiting", "sleeping"];
const OWN_FPS = {idle: 4, working: 4, waiting: 2, sleeping: 1};
const BANG = ["K", "K", "K", ".", "K"], DROP = [".K.", ".K.", "KKK", "KKK", ".K."];
const ZEE = ["KKKKK", "...K.", "..K..", ".K...", "KKKKK"];
const WARN = "#FFA049", SECONDARY = "#A8A8B0", SWEAT = "#7FC4FF";
const NAMES = {claude: "Claude", codex: "Codex", agy: "AGY (Gemini)", pi: "Pi", bee: "Swarm bee"};
const NOTES = {claude: "Own art, provider family", codex: "Own art, provider family", agy: "Own art, provider family",
               pi: "Own art, provider family", bee: "Own art, swarm's turn; works in a swarm of up to 3"};
const H = 48, DPR = 2;  // the art height in CSS px, like the open card
const snap = v => Math.round(v * 2) / 2;
const strained = document.getElementById("strained"), still = document.getElementById("still");

function sprite(ctx, rows, color, x, y, cell) {
  rows.forEach((line, r) => [...line].forEach((k, c) => {
    const fill = color(k);
    if (fill) { ctx.fillStyle = fill; ctx.fillRect(x + c * cell, y + r * cell, cell, cell); }
  }));
}

function pose(art, state, t) {
  if (state === "working") return {rows: art.working[t % art.working.length], bob: t % 2 ? 1 / 3 : 0};
  if (state === "idle") return {rows: t % 14 === 13 ? art.blink : art.idle, bob: [0, 1, 2, 3, 2, 1][t % 6] / 3};
  return {rows: art[state], bob: 0};
}

function cue(state, seconds, ownZ) {
  if (state === "waiting") return seconds === null || Math.floor(seconds * 2) % 2 === 0 ? [BANG, WARN, 0, false] : null;
  if (state === "sleeping") return ownZ ? null : [ZEE, SECONDARY, seconds === null ? 1 : Math.floor(seconds) % 3, false];
  return strained.checked ? [DROP, SWEAT, 2 - (seconds === null ? 0 : Math.floor(seconds) % 2), true] : null;
}

function draw(canvas, m, state, t, seconds) {
  const ctx = canvas.getContext("2d"), W = canvas.width / DPR, Hc = canvas.height / DPR;
  ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, W, Hc);
  let box;
  if (m.img) {
    const row = STATES.indexOf(state), n = m.frames[row], cols = Math.max(...m.frames);
    const scale = Math.min(H / m.h, W / m.w), w = snap(m.w * scale), h = snap(m.h * scale);
    box = {x: snap((W - w) / 2), y: snap((Hc - h) / 2), w, h};
    const f = (t ?? 0) % n;
    ctx.drawImage(m.img, f * m.w, row * m.h, m.w, m.h, box.x, box.y, w, h);
  } else {
    const bees = m.id === "bee" && state === "working" ? 3 : 1;
    const cell = Math.min(snap(H / 10), snap(W / (12 * bees))), w = 12 * cell, h = 10 * cell;
    const left = snap((W - w * bees) / 2);
    let minY = Infinity;
    for (let b = 0; b < bees; b++) {
      const p = pose(m, state, (t ?? 0) + b);
      const lift = bees > 1 && b === 1 ? Math.min(cell * 2, Math.max((Hc - h) / 2 - cell / 3, 0)) : 0;
      const y = snap((Hc - h) / 2 - p.bob * cell - lift);
      sprite(ctx, p.rows, k => m.palette[k], left + b * w, y, cell);
      minY = Math.min(minY, y);
    }
    box = {x: left, y: minY, w: w * bees, h};
  }
  const c = cue(state, seconds, m.ownZ);
  if (c) {
    const [glyph, color, rise, onLeft] = c, cell = Math.max(Math.round(H / 24), 1);
    const gw = glyph[0].length * cell, gh = glyph.length * cell;
    const x = onLeft ? Math.max(box.x - gw / 2, 0) : Math.min(box.x + box.w - gw / 2, W - gw);
    const y = Math.max(box.y - gh, 0) + (2 - rise) * cell;
    sprite(ctx, glyph, k => k === "K" ? color : null, snap(x), snap(y), cell);
  }
}

const cast = [
  ...OWN.map(a => ({...a, name: NAMES[a.id] ?? a.id, credit: NOTES[a.id] ?? "Own art", ownZ: false,
                    fps: STATES.map(s => OWN_FPS[s])})),
  ...GUESTS.map(g => { const img = new Image(); img.src = "data:image/png;base64," + g.png; return {...g, img}; }),
];
const views = [];
const grid = document.getElementById("grid");
for (const m of cast) {
  const card = document.createElement("div");
  card.className = "card";
  card.innerHTML = "<h2></h2><small></small><div class=states></div>";
  card.querySelector("h2").textContent = m.name;
  card.querySelector("small").textContent = m.credit;
  STATES.forEach((state, i) => {
    const cell = document.createElement("div"), canvas = document.createElement("canvas");
    cell.className = "state";
    cell.append(canvas, state);
    card.querySelector(".states").append(cell);
    views.push({canvas, m, state, fps: m.fps[i]});
  });
  grid.append(card);
}

function frame(now) {
  const seconds = now / 1000;
  for (const v of views) {
    const r = v.canvas.getBoundingClientRect();
    if (v.canvas.width !== Math.round(r.width * DPR)) { v.canvas.width = Math.round(r.width * DPR); v.canvas.height = Math.round(r.height * DPR); }
    if (v.m.img && !v.m.img.complete) continue;
    draw(v.canvas, v.m, v.state, still.checked ? null : Math.round(seconds * v.fps), still.checked ? null : seconds);
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
</script>
"""


def main() -> None:
    out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / ".build/mascots.html"
    own, sheets = own_art(), guests()
    data = lambda v: json.dumps(v).replace("</", "<\\/")  # keeps the JSON inside the script tag
    page = (PAGE.replace("OWN_JSON", data(own)).replace("GUESTS_JSON", data(sheets))
            .replace("COUNT", str(len(own) + len(sheets))))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(page)
    print(out, f"{len(own)} own, {len(sheets)} guests", file=sys.stderr)


if __name__ == "__main__":
    main()
