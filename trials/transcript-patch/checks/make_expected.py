"""Builds patch_inputs.json and patch_expected.json ONLY from the sealed ORIGINAL captures of lane-d-patch-4 (c8ca837e public
transcript.ts / changes.ts, Node 22 built-in transform, fuse.js pinned). No value comes from Rust or from a candidate.
Strings are decoded from exact UTF-16 units and must be well formed (this first adapter is valid UTF-8 only). Numbers stay
JSON numbers as the ORIGINAL JSON.stringify wrote them (non-finite becomes null). Source hashes are checked before use.
usage: python3.12 -B -I make_expected.py   (refuses to overwrite)"""
import hashlib, json, os

P4 = "/tmp/sw-u07-check.bwsayl8l/lane-d-patch-4"
HERE = os.path.dirname(os.path.abspath(__file__))
SRC = {f"{P4}/out/orig-1.stdout": "48df3443492afc67277e15d464108d37063b041e779622637904d266ac8d94fc",
       f"{P4}/scripts/inputs4-legit.json": "d3de903801ede575960b52f338fe7352e1b6e848cadfc0832642237973308759",
       f"{P4}/scripts/inputs4-over.json": "54f6674b385eef7a71c4e9c94879870686acf5aef39ead91ea8994881936cd12",
       f"{P4}/scripts/inputs4-inf.json": "b80645c49e8d23071b5900e8b628bb21bd9f26450c88d7815c95bd23e8543038"}
for p, h in SRC.items():
    assert hashlib.sha256(open(p, "rb").read()).hexdigest() == h, p


def dec(v):
    if isinstance(v, dict) and set(v) == {"u16", "len"}:
        s = "".join(chr(int(v["u16"][i:i + 4], 16)) for i in range(0, len(v["u16"]), 4))
        assert len(s) == v["len"]
        s.encode("utf-8")  # raises on a lone surrogate: out of scope for this adapter
        return s
    if isinstance(v, list):
        return [dec(x) for x in v]
    if isinstance(v, dict):
        return {k: dec(x) for k, x in v.items()}
    return v


orig = json.load(open(f"{P4}/out/orig-1.stdout"))
groups = {g: json.load(open(f"{P4}/scripts/inputs4-{g}.json")) for g in ("legit", "over", "inf")}
patch, comment = {}, {}
for g in groups.values():
    for c in g["patch"]:
        patch[c["id"]] = {k: (dec(r["value"]) if "value" in r else None) for k, r in orig["patch"][c["id"]].items()}
        assert all("value" in r for r in orig["patch"][c["id"]].values()), c["id"]
    for c in g["comment"]:
        assert "value" in orig["comment"][c["id"]], c["id"]
        comment[c["id"]] = dec(orig["comment"][c["id"]]["value"])
expected = {"about": "ORIGINAL public values only (sealed lane-d-patch-4 orig-1). Compare as JS receiver: numbers are binary64, null/absent/arrays/strings exact.",
            "sources": {os.path.basename(p): h for p, h in SRC.items()}, "patch": patch, "comment": comment}
with open(f"{HERE}/patch_expected.json", "x", encoding="utf-8") as f:
    json.dump(expected, f, indent=1, ensure_ascii=True)
    f.write("\n")
with open(f"{HERE}/patch_inputs.json", "x", encoding="utf-8") as f:
    json.dump({"about": "CLI task inputs per group; each group is one 'CANDIDATE task INPUTS --composed [] --out DIR' run (render/find empty).", "groups": groups}, f, indent=1, ensure_ascii=True)
    f.write("\n")
print("ok", sorted(patch), sorted(comment))
