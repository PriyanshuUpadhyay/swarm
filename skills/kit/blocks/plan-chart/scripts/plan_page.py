"""Draw plan.md as plan.html, the one page the owner reviews: plan_chart.py runs it; see its docstring."""

import hashlib
import html
import re
import sys
from pathlib import Path

KIT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(KIT / "references"))  # python3 -I does not add it
from step_brief import briefs  # noqa: E402
from block_resolver import frontmatter, read_parents, resolve_blocks  # noqa: E402

ASSETS = Path(__file__).resolve().parents[1] / "assets"


def parents():
    return read_parents({p.parent.name: p for p in sorted((KIT / "blocks").glob("*/SKILL.md"))})


CSS = """
:root{--bg:#f7f6f2;--panel:#fff;--ink:#1d1d1b;--muted:#5f5d57;--line:#d9d6cc;--block:#2f6fde;--step:#1d8a5b;--warn:#b26b00;
--block-bg:#e7efff;--step-bg:#e2f4ec;--warn-bg:#fff3dd;--code:#f0eee7}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--bg:#141413;--panel:#1e1e1c;--ink:#ecebe6;--muted:#a6a39b;
--line:#3a3935;--block:#7aa7ff;--step:#5fd3a0;--warn:#f0b450;--block-bg:#1d2a44;--step-bg:#16342a;--warn-bg:#3a2d14;--code:#2a2926}}
:root[data-theme="dark"]{--bg:#141413;--panel:#1e1e1c;--ink:#ecebe6;--muted:#a6a39b;--line:#3a3935;--block:#7aa7ff;--step:#5fd3a0;
--warn:#f0b450;--block-bg:#1d2a44;--step-bg:#16342a;--warn-bg:#3a2d14;--code:#2a2926}
*{box-sizing:border-box}html{scroll-padding-top:56px}
html,body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.6 Inter,system-ui,sans-serif}
main{max-width:980px;margin:0 auto;padding:0 16px 80px}
nav{position:sticky;top:0;z-index:5;background:var(--bg);border-bottom:1px solid var(--line);margin:0 -16px;padding:10px 16px;
display:flex;gap:14px;overflow-x:auto;font-size:14px;white-space:nowrap}
nav a{color:var(--muted);text-decoration:none}nav a:hover{color:var(--ink)}
.toggle{margin-left:auto;background:none;border:1px solid var(--line);color:var(--ink);border-radius:6px;padding:2px 8px;cursor:pointer;font:inherit}
h1{font-size:clamp(26px,4.5vw,38px);line-height:1.15;margin:24px 0 12px;letter-spacing:-.02em}
h2{font-size:24px;margin:52px 0 8px;letter-spacing:-.01em}h3{font-size:17px;margin:0 0 6px}p{margin:8px 0}
.muted{color:var(--muted);font-size:14px}.lede{font-size:18px;color:var(--muted);max-width:760px}
.strip,.answer{display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:12px;margin:20px 0}
.card{background:var(--panel);border:1px solid var(--line);border-radius:12px;padding:14px 16px}
.card .k{font-size:12px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted)}
.card .v{font-size:17px;font-weight:600;margin:4px 0;line-height:1.35}
.stat b{display:block;font-size:26px;line-height:1.2}.stat span{color:var(--muted);font-size:13px}
.legend{display:flex;flex-wrap:wrap;gap:16px;font-size:14px;color:var(--muted);margin:4px 0 0}
.legend span::before{content:"";display:inline-block;width:12px;height:12px;border-radius:3px;margin-right:6px;vertical-align:-1px;background:var(--c)}
.callout{border-radius:10px;padding:12px 16px;background:var(--warn-bg);border:1px solid var(--warn)}
.part{background:var(--panel);border:1px solid var(--line);border-left:5px solid var(--c);border-radius:10px;padding:14px 16px;margin:14px 0}
.part.block{--c:var(--block)}.part.step{--c:var(--step)}
.part ul{margin:6px 0 0;padding-left:20px}.part li{margin:3px 0}
.tag{display:inline-block;font-size:11px;font-weight:700;letter-spacing:.05em;text-transform:uppercase;border-radius:999px;padding:1px 9px;margin-right:6px;vertical-align:2px}
.tag.known{background:var(--step-bg);color:var(--step);border:1px solid var(--step)}
.tag.check{background:var(--warn-bg);color:var(--warn);border:1px solid var(--warn)}
.src{font-size:13px;color:var(--muted);margin-right:6px}.src::after{content:" ·"}
.so{border-top:1px dashed var(--line);margin-top:10px;padding-top:8px}.so b{color:var(--c)}
.stepid{float:right;font-size:13px}.stepid code{color:var(--muted)}
.chips{display:flex;flex-wrap:wrap;gap:6px;margin:6px 0}.chips code{border:1px solid var(--line)}
li.sb{list-style:none;margin-left:-20px;margin-top:8px;font-size:14px;color:var(--muted)}
code{font:13.5px "JetBrains Mono",ui-monospace,monospace;background:var(--code);padding:1px 5px;border-radius:4px;overflow-wrap:anywhere}
pre{background:var(--code);border-radius:8px;padding:10px 12px;overflow-x:auto}pre code{padding:0;background:none}
a{color:var(--block)}
.tablewrap{overflow-x:auto;border:1px solid var(--line);border-radius:10px;margin:10px 0}
table{width:100%;border-collapse:collapse;font-size:14.5px;background:var(--panel)}
th,td{border-bottom:1px solid var(--line);padding:8px 10px;text-align:left;vertical-align:top}
td:first-child code{white-space:nowrap}th{font-size:12px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted)}
details{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:10px 14px;margin:14px 0}
summary{cursor:pointer;font-weight:600}
.chart,.figure{background:#fafafa;color:#1d1d1b;border:1px solid var(--line);border-radius:12px;padding:12px;margin:12px 0;overflow-x:auto}
.pele{--pele-bg:#fafafa;--pele-font:Inter,system-ui,sans-serif}
"""


def inline(text):
    """Escape text, then show each `code` span as <code>, and **bold**, *italic*, and http(s) links."""
    out = []
    for i, part in enumerate(re.split(r"(`[^`]+`)", text)):
        if i % 2:
            out.append(f"<code>{html.escape(part[1:-1])}</code>")
            continue
        part = html.escape(part, quote=False)
        part = re.sub(r"\[([^\]]+)\]\((https?://[^)\s\"]+)\)", r'<a href="\2">\1</a>', part)
        part = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", part)
        part = re.sub(r"(?<![\w*])\*(?!\s)(.+?)\*(?!\w)", r"<em>\1</em>", part)
        out.append(part)
    return "".join(out)


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", re.sub(r"<[^>]+>", "", text).lower()).strip("-")


# An html fence is a drawn diagram (blocks/html-diagram), so it may use only these tags and attributes; anything
# else could run code or restyle the page the owner approves.
TAGS = {"figure", "figcaption", "div", "span", "p", "ol", "ul", "li", "b", "strong", "i", "em", "code", "small", "br",
        "table", "thead", "tbody", "tr", "th", "td", "h3", "h4"}
ATTRS = re.compile(r"(?:class|style|id|title|role|aria-[\w-]+|data-[\w-]+)$")


def unsafe(text):
    """The first tag or attribute of an html fence that a drawn diagram may not use, or None."""
    for tag in re.finditer(r"<\s*(/?)\s*([^\s/>]*)([^>]*)>?", text):
        if tag[2].lower() not in TAGS:
            return tag[0]
        for attr in re.findall(r"([^\s=/\"']+)\s*=", tag[3]):
            if not ATTRS.match(attr.lower()):
                return f"{attr}="
    if re.search(r"url\s*\(|expression\s*\(|@import", text, re.I):
        return "url(" if "url" in text.lower() else "expression("
    return None


ITEM = re.compile(r"( *)(- |\d+\. )(.*)")


def items(lines, i):
    """(html, next i) for the list that starts at lines[i]; a deeper indent opens a nested list."""
    depth = len(ITEM.match(lines[i])[1])
    tag = "ol" if ITEM.match(lines[i])[2][0].isdigit() else "ul"
    out = []
    while i < len(lines) and (m := ITEM.match(lines[i])) and len(m[1]) >= depth:
        if len(m[1]) > depth:
            inner, i = items(lines, i)
            out[-1] = out[-1].removesuffix("</li>") + inner + "</li>"
            continue
        out.append(f"<li>{inline(m[3])}</li>")
        i += 1
    return f"<{tag}>{''.join(out)}</{tag}>", i


def convert(md):
    """HTML for the fixed plan.md shape: headings, paragraphs, lists, tables, fences, and the <details> fold.
    A `mermaid` fence becomes a chart, and an `html` fence, a diagram the agent drew, goes in as written."""
    lines, out, i = md.splitlines(), [], 0
    while i < len(lines):
        line = lines[i]
        if m := re.match(r"`{3,}", line):
            lang, body = line[len(m[0]):].strip(), []
            i += 1
            while i < len(lines) and not (lines[i].strip().startswith(m[0]) and set(lines[i].strip()) == {"`"}):
                body.append(lines[i])
                i += 1
            text = "\n".join(body)
            if lang == "html" and (bad := unsafe(text)):
                raise SystemExit(f"an html fence holds `{bad}`; a drawn diagram uses only plain tags with class and style")
            out.append(f'<div class="chart">{html.escape(text)}</div>' if lang == "mermaid"
                       else f'<div class="figure">{text}</div>' if lang == "html"
                       else f"<pre><code>{html.escape(text)}</code></pre>")
        elif m := re.match(r"(#{1,3}) (.*)", line):
            level, text = len(m[1]), inline(m[2])
            out.append(f'<h{level} id="{slug(text)}">{text}</h{level}>')
        elif line.startswith("|"):
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                if not lines[i].startswith("|---"):
                    rows.append([c.strip().replace("\\|", "|") for c in re.split(r"(?<!\\)\|", lines[i].strip().strip("|"))])
                i += 1
            head, *body = rows or [[]]
            out.append('<div class="tablewrap"><table><thead><tr>' + "".join(f"<th>{inline(c)}</th>" for c in head)
                       + "</tr></thead><tbody>" + "".join("<tr>" + "".join(f"<td>{inline(c)}</td>" for c in r) + "</tr>"
                                                          for r in body) + "</tbody></table></div>")
            continue
        elif ITEM.match(line):
            text, i = items(lines, i)
            out.append(text)
            continue
        elif line.strip() in ("<details>", "</details>"):
            out.append(line.strip())
        elif m := re.fullmatch(r"<summary>(.*)</summary>", line.strip()):
            out.append(f"<summary>{inline(m[1])}</summary>")
        elif line.strip() and not line.startswith("<!--"):
            out.append(f"<p>{inline(line)}</p>")
        i += 1
    return "\n".join(out)


def ledger_rows(ledger, status):
    """[(block, why)] of one Status, each part right under its parent when the parent has the same Status."""
    part_of = {part: parent for parent, parts in parents().items() for part in parts}
    names = {b for b, (s, _) in ledger.items() if s == status}
    under = {b: part_of[b] for b in names if part_of.get(b) in names}
    return [(b, ledger[b][1]) for b in sorted(names, key=lambda b: (under.get(b, b), b in under, b))]


def skill_blocks(rows, ledger):
    """The Skill blocks section as Markdown: Used and Offered from the blocks table, each with its SKILL.md
    description, then every other kit block folded."""
    files = sorted((KIT / "blocks").glob("*/SKILL.md"))
    desc = {f.parent.name: frontmatter(f.read_text(encoding="utf-8"), f.parent.name)[0].get("description", "")
            for f in files}
    does = {b: d.split(" Use when ")[0] for b, d in desc.items()}
    when = {b: d.partition(" Use when ")[2].rstrip(".") or "—" for b, d in desc.items()}
    selected = resolve_blocks(None, {r[0]: r[3] for r in rows}, "default", blocks_dir=KIT / "blocks", pack=False)
    steps = {b: [n for n, blocks in selected.items() if b in blocks] for b in desc}
    cell = lambda text: text.replace("|", "\\|")
    used, offered = ledger_rows(ledger, "Chosen"), ledger_rows(ledger, "Offered")
    rest = [b for b in desc if b not in ledger]
    out = ["## Skill blocks", "",
           f"A skill block is one small rule file in the kit that a step follows. The kit has {len(desc)}. "
           f"This plan uses {len(used)}, offers {len(offered)} more, and leaves {len(rest)} out. "
           'Say "add X" or "drop X" to change the plan.', "",
           f"### Used in this plan ({len(used)})", "", "| Block | What it does | Why it is here | Step |", "|---|---|---|---|"]
    out += [f"| `{b}` | {cell(does.get(b, '(not in the kit)'))} | {cell(why)} | {', '.join(steps.get(b, []))} |" for b, why in used]
    if offered:
        out += ["", f"### Offered, left off ({len(offered)})", "", "| Block | What it does | Why it is off |", "|---|---|---|"]
        out += [f"| `{b}` | {cell(does.get(b, '(not in the kit)'))} | {cell(why)} |" for b, why in offered]
    out += ["", "<details>", f"<summary>Other blocks in the kit ({len(rest)}), and when each one is used</summary>", "",
            "| Block | What it does | Used when |", "|---|---|---|"]
    out += [f"| `{b}` | {cell(does[b])} | {cell(when[b])} |" for b in rest]
    return "\n".join(out + ["", "</details>", ""])


def dress(body):
    """C's look: question cards, a problem callout, block and step cards with tags and search chips."""
    parts = re.split(r'(<h2 id="[^"]+">.*?</h2>)', body)
    for n in range(2, len(parts), 2):
        head, part = parts[n - 1], parts[n]
        if 'id="questions-this-plan-answers"' in head:
            part = re.sub(r"<ol>(.*?)</ol>", lambda m: '<div class="answer">' + "".join(
                f'<div class="card"><div class="k">Question {i}</div><div class="v">{q}</div></div>'
                for i, q in enumerate(re.findall(r"<li>(.*?)</li>", m[1]), 1)) + "</div>", part, flags=re.S)
        elif 'id="the-problem-in-one-example"' in head:
            part = part.replace("<p>", '<p class="callout">', 1)
        elif 'id="building-blocks"' in head or 'id="steps"' in head:
            kind = "block" if "building" in head else "step"
            chunks = re.split(r"(?=<h3 )", part)
            part = chunks[0] + "".join(f'<section class="part {kind}">{c}</section>' for c in chunks[1:])
            part = re.sub(r"<p>Known \(([^)]*)\)\. ", r'<p><span class="tag known">Known</span><span class="src">\1</span> ', part)
            part = re.sub(r"<p>To check in step (\d+)[,.]? ?", r'<p><span class="tag check">To check · step \1</span> ', part)
            part = re.sub(r"<p>So in this (plan|design): ", r'<p class="so"><b>So in this \1:</b> ', part)
            part = re.sub(r"<p>Step: (<code>[^<]+</code>)</p>", r'<span class="stepid">\1</span>', part)
            part = re.sub(r"<li>(Seed searches[^<]*)<ul>(.*?)</ul></li>", lambda m: f'<li>{m[1]}<div class="chips">'
                          + "".join(s if s.startswith("<code>") else f"<code>{s}</code>" for s in re.findall(r"<li>(.*?)</li>", m[2]))
                          + "</div></li>", part)
            part = re.sub(r"<li>Skill blocks: (.*?)</li>", r'<li class="sb">Skill blocks: \1</li>', part)
        parts[n] = part
    return "".join(parts)


def render(text, rows, ledger):
    """plan.html: the whole plan.md as one page in the readable layout, with the Skill blocks section."""
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()  # start_plan.py binds the approved page to this plan.md
    blocks = skill_blocks(rows, ledger)
    old = re.compile(r"^## Blocks considered\n\s*(?:\|.*\n?)+", re.M)  # a plan in the older layout
    text = (old.sub(lambda _: blocks, text, count=1) if old.search(text)
            else text.replace("\n## Done when\n", f"\n{blocks}\n## Done when\n", 1) if "\n## Done when\n" in text
            else f"{text}\n{blocks}")
    body = dress(convert(text))
    title = re.search(r"<h1[^>]*>(.*?)</h1>", body)
    body = body.replace(title[0], "", 1) if title else body
    body = re.sub(r"^\s*<p>(Preset: [^<]*)</p>", r'<p class="muted">\1</p>', body)
    lede = re.search(r"<p>(.*?)</p>", body.split("<h2", 1)[0])
    body = body.replace(lede[0], "", 1) if lede else body
    nav = "".join(f'<a href="#{i}">{t}</a>' for i, t in re.findall(r'<h2 id="([^"]+)">(.*?)</h2>', body))
    stats = [(len(re.findall(r"^### Block", text, re.M)), "building blocks"), (len(briefs(text)), "steps you approve"),
             (sum(s == "Chosen" for s, _ in ledger.values()), "skill blocks used"),
             (sum(s == "Offered" for s, _ in ledger.values()), "skill blocks offered")]
    strip = "".join(f'<div class="card stat"><b>{n}</b><span>{label}</span></div>' for n, label in stats if n)
    legend = ('<div class="legend"><span style="--c:var(--block)">building block</span><span style="--c:var(--step)">step you '
              'approve</span><span style="--c:var(--warn)">still to check</span></div>')
    heading = title[1] if title else "Plan"
    return ('<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,'
            f'initial-scale=1"><meta name="plan-md-sha256" content="{digest}"><title>{re.sub("<[^>]+>", "", heading)}</title><style>{CSS}</style></head><body><main>'
            f'<nav>{nav}<button class="toggle" title="light or dark" onclick="const r=document.documentElement;'
            "r.dataset.theme=(r.dataset.theme||(matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light'))==='dark'"
            "?'light':'dark'\">◐</button></nav>"
            f'<h1>{heading}</h1>{f"""<p class="lede">{lede[1]}</p>""" if lede else ""}<div class="strip">{strip}</div>'
            f"{legend}{body}</main><script>{(ASSETS / 'pele.global.js').read_text(encoding='utf-8')}</script>"
            "<script>document.querySelectorAll('.chart').forEach(c=>pele.mount(c,c.textContent))</script></body></html>\n")
