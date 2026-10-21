# Implement test visual checks cases and helpers.
from test_implement_run_support import *


def test_block_checks(tmp):
    """Check (script) lines become step todos; Check (judge) lines go to 06-review only, and done
    refuses a checked block todo with no evidence even when its command holds a colon."""
    out, folder, _, run = blocks_start(tmp, "d6", "- build: block:html-diagram\n")
    assert out.returncode == 0, out.stderr
    build, review = step_text(folder, "05-build"), step_text(folder, "06-review")
    todo = build.split(implement_run.TODO_HEAD)[1]
    script = (r"- [ ] block html-diagram script: `rg -c -g '*.html' '<figure(\s|>|$)' site` prints a count of 1 or more, and "
              r"`rg -U --pcre2 -n -g '*.html' '<figure[\s>](?:(?!</figure>|<figcaption)[\s\S])*</figure>|<figure[\s>](?:(?!</figure>)[\s\S])*?"
              r"<figcaption(?:(?!</figure>)[\s\S])*?<figcaption' site` prints nothing and exits 1: the page has a figure, "
              "and each figure has exactly one caption.: ")
    assert script + "\n" in todo, todo
    assert "script: none:" not in todo, "a `- none:` line says why a block has no script check; it is no todo"
    judge = "At each width that `blocks/web-ui-basics/SKILL.md` names, does each arrow still point from its source to its target?"
    assert judge not in build, "a judge check must not be a build todo"
    # The judge section sits after the rules and before the todo head, so done still finds the todo list.
    rules_at, judge_at, todo_at = (review.index(h) for h in (implement_run.RULES_HEAD, implement_run.JUDGE_HEAD, implement_run.TODO_HEAD))
    assert rules_at < judge_at < todo_at and judge in review[judge_at:todo_at], review
    assert implement_run.step_run.todo_scan(review)[0] == todo_at, review
    assert "block base-rules judge: For a page or doc built from a repo, does every fact trace to a line in docs/spec.md?" in review, review
    assert review.count("block base-rules judge:") == 1, "base has one source check, once per run"
    f = folder / "05-build.md"
    fill(f)
    text = f.read_text(encoding="utf-8")
    f.write_text(text.replace(script.replace("- [ ]", "- [x]").rstrip() + " evidence", script.replace("- [ ]", "- [x]").rstrip()), encoding="utf-8")
    refused = run("done", folder, "05-build", claim(folder, "05-build"))
    assert refused.returncode == 1 and "no evidence: - [x] block html-diagram script:" in refused.stderr, refused.stderr


def test_html_diagram_check(tmp):
    """The html-diagram script check, run as written, passes a page only when it has a figure and each
    figure has exactly one caption, also when the figure tag's attributes start on the next line. DESIGN.md
    prose about `<figure>` beside the page counts for nothing."""
    pages = {'<figure\n  class="x"><p>a</p><figcaption>A</figcaption></figure>\n': True,
             '<figure class="x"><figcaption>A</figcaption></figure>\n<figure><figcaption>B</figcaption></figure>\n': True,
             '<figure\n  class="x"><p>a</p></figure>\n': False,
             '<figure><p>a</p></figure>\n': False,
             '<figure><figcaption>A</figcaption><figcaption>B</figcaption></figure>\n': False,
             '<figure-card><figcaption>A</figcaption></figure-card>\n': False,
             '<p>no diagram</p>\n': False}
    for index, (html, good) in enumerate(pages.items()):
        site = Path(tmp, f"diagram-{index}")
        site.mkdir()
        (site / "index.html").write_text(html, encoding="utf-8")
        (site / "DESIGN.md").write_text("Wrap each diagram in <figure> with one <figcaption>.\n", encoding="utf-8")
        counted, captioned = block_rg("html-diagram", site)
        assert captioned.returncode in (0, 1) and counted.returncode in (0, 1), (captioned.stderr, counted.stderr)
        passed = (counted.returncode == 0 and sum(int(n.rsplit(":", 1)[-1]) for n in counted.stdout.splitlines()) >= 1
                  and captioned.returncode == 1 and not captioned.stdout)
        assert passed == good, (html, counted, captioned)
        assert "DESIGN.md" not in counted.stdout + captioned.stdout, (counted, captioned)


def test_design_tokens_colors(tmp):
    """The design-tokens color check finds each color literal in a declaration (also before a comma or on a
    later line of a value), and not a selector that looks like a hex color. A literal in the :root token block,
    or in a :root that is all of @media (prefers-color-scheme: dark), is no hit; a :root anywhere else is."""
    site = Path(tmp, "colors-site")
    site.mkdir()
    (site / "index.html").write_text("<p>colors</p>\n", encoding="utf-8")  # the block's other checks read it
    (site / "a.css").write_text("\n".join([
        ":root { --bg: #fff; --fg: oklch(0.2 0 0); }",  # 1 the token block
        "#face .x { color: var(--fg); }",  # 2 selector
        "#bad,",  # 3 selector list
        "#cafe { color: var(--bg); }",  # 4 selector
        "a { background: linear-gradient(#fff, #eee, red); }",  # 5
        "b { box-shadow: 0 0 1px #000, 0 0 2px var(--bg); }",  # 6
        "c {",  # 7
        "  box-shadow:",  # 8
        "    0 0 1px #abc,",  # 9
        "    0 0 2px var(--fg);",  # 10
        "}",  # 11
        "d { border: 1px solid #abc }",  # 12
        "e { color: color-mix(in srgb, #fff 50%, red); }",  # 13
        "f { color: lab(50% 0 0); }",  # 14
        "@media (min-width: 40em) { #dead { color: var(--fg); } }",  # 15 selector inside a block
        ".card { color: #fff; &:hover { color: var(--fg); } }",  # 16 a hex before a nested rule
        ".card { &#fff-id { color: var(--fg); } }",  # 17 a nested selector that looks like a hex
        "@media (prefers-color-scheme: dark) {",  # 18 the dark-mode token block
        "  :root { --bg: #111; --fg: oklch(0.9 0 0); }",  # 19
        "}",  # 20
        "@media (min-width: 40em) { :root { --bg: #222; } }",  # 21 a :root in another media query
        "g { color: #123456; }",  # 22 a stray literal
        ":root{--bg:#fff}@media (prefers-color-scheme: dark){:root{--bg:#111}}",  # 23 both blocks, minified
        "@media (prefers-color-scheme: dark) { :root { --bg: #111; } h { color: #333; } }",  # 24 more than :root
    ]) + "\n", encoding="utf-8")
    colors = block_rg("design-tokens", site)[0]
    assert colors.returncode == 0, colors.stderr
    lines = sorted(int(line.split(":")[1]) for line in colors.stdout.splitlines())
    assert lines == [5, 6, 9, 12, 13, 14, 16, 21, 22, 24], (lines, colors.stdout)


def test_diagram_shot_check(tmp):
    """Diagram images have each width from the owner and match their named width."""
    root = Path(tmp, "shot-check")
    text = (implement_run.BLOCKS / "diagram-check" / "SKILL.md").read_text(encoding="utf-8")
    command = re.search(r"`([^`]+)`", text.split("## Check (script)\n", 1)[1])[1].replace("{{shots}}", "shots")
    owner = (implement_run.BLOCKS / "web-ui-basics" / "SKILL.md").read_text(encoding="utf-8")
    widths = [int(w) for w in re.findall(r"\d+", re.search(r"Check widths are ([^.]+)", owner)[1])]
    png = lambda width: (b"\x89PNG\r\n\x1a\n" + (13).to_bytes(4, "big") + b"IHDR" + width.to_bytes(4, "big")
                         + (812).to_bytes(4, "big") + bytes([8, 6, 0, 0, 0]) + bytes(4))
    def ok():
        lines = subprocess.run(command, shell=True, cwd=root, capture_output=True, text=True).stdout.splitlines()
        return bool(lines) and all(
            (match := re.search(r"-(\d+)\.png:.*PNG image data, (\d+) x", line))
            and int(match[1]) in widths and match[1] == match[2] for line in lines)
    (root / "shots").mkdir(parents=True)
    assert not ok(), "no shot must fail"
    for width in widths:
        for diagram in ("one", "two"):
            (root / "shots" / f"diagram-{diagram}-{width}.png").write_bytes(png(width))
    assert ok(), "diagram shots at all owner widths must pass"
    for bad in (b"", png(widths[-1] + 1)):
        (root / "shots" / f"diagram-two-{widths[-1]}.png").write_bytes(bad)
        assert not ok(), bad


def test_design_tokens_page_code_colors(tmp):
    """The design-tokens page-code check finds a color function in JS and TS files, and not a quoted hex, which
    could be an id selector such as "#feed" (the judge line asks about quoted hex colors), nor a private field;
    a page whose code reads only tokens prints nothing."""
    site = Path(tmp, "code-colors-site")
    site.mkdir()
    (site / "index.html").write_text("<p>colors</p>\n", encoding="utf-8")  # the block's other checks read it
    (site / "main.js").write_text("\n".join([
        'el.style.color = "#ff0000";',  # 1 a quoted hex, for the judge
        'ctx.fillStyle = "rgb(1,2,3)";',  # 2
        'document.querySelector("#main");',  # 3 not a hex
        "class A { #abc = 1; }",  # 4 a private field
        "el.style.color = 'var(--fg)';",  # 5
        'document.querySelector("#feed");',  # 6 an id selector made of hex letters
    ]) + "\n", encoding="utf-8")
    (site / "view.tsx").write_text("const c = `#abc`;\nconst d = hsl(1, 2, 3);\n", encoding="utf-8")
    code = block_rg("design-tokens", site)[2]
    hits = sorted(line.split(":", 2)[0].rsplit("/", 1)[1] + ":" + line.split(":", 2)[1] for line in code.stdout.splitlines())
    assert hits == ["main.js:2", "view.tsx:2"], (hits, code.stdout)
    (site / "main.js").write_text("el.style.color = 'var(--fg)';\nclass A { #abc = 1; }\n", encoding="utf-8")
    (site / "view.tsx").unlink()
    code = block_rg("design-tokens", site)[2]
    assert code.returncode == 1 and not code.stdout, code


def test_design_tokens_dashes(tmp):
    """The design-tokens dash check finds an em or en dash as a character, a named entity, or a numeric
    entity, and passes a page with only hyphens and other entities."""
    site = Path(tmp, "dash-site")
    site.mkdir()
    dashes = ("—", "–", "&mdash;", "&ndash;", "&#8212;", "&#8211;", "&#x2014;", "&#X2013;",
              "&#08212;", "&#x002014;", "&#0008211;", "&#X02013;")
    for text, found in [(d, True) for d in dashes] + [("a - b &amp; &#8210; &#821; &#08210; &#x02012;", False), ("", False)]:
        (site / "index.html").write_text(f"<p>tool {text} text</p>\n", encoding="utf-8")
        out = block_rg("design-tokens", site)[1]
        assert (out.stdout.strip() != "") == found, (text, out)


def test_web_ui_basics_checks(tmp):
    """The web-ui-basics rg checks read the page files, not DESIGN.md prose beside them."""
    site = Path(tmp, "basics-site")
    site.mkdir()
    (site / "DESIGN.md").write_text("Never use outline: none. prefers-reduced-motion is nice.\n", encoding="utf-8")
    (site / "index.html").write_text("<h1>A</h1>\n", encoding="utf-8")
    (site / "style.css").write_text("h1 { color: red; }\n", encoding="utf-8")
    banned, motion = block_rg("web-ui-basics", site)[:2]
    assert banned.returncode == 1 and not banned.stdout, banned
    assert motion.returncode == 1 and not motion.stdout, "DESIGN.md prose must not count as handling reduced motion"
    (site / "style.css").write_text("@media (prefers-reduced-motion: reduce) { * { animation: none; } }\n", encoding="utf-8")
    banned, motion = block_rg("web-ui-basics", site)[:2]
    assert motion.returncode == 0 and "style.css" in motion.stdout, motion
    # The banned-pattern check hits ease-in at a line end and maximum-scale=1 (or 1.0), not 1.5 or ease-in-out.
    (site / "style.css").write_text("a { transition: opacity 200ms ease-in\n}\nb { transition: color 200ms ease-in-out; }\n",
                                    encoding="utf-8")
    (site / "index.html").write_text('<meta content="maximum-scale=1.5">\n<meta content="maximum-scale=1">\n'
                                     '<meta content="maximum-scale=1.0, width=device-width">\n', encoding="utf-8")
    hits = [line.split(":", 1)[1] for line in block_rg("web-ui-basics", site)[0].stdout.splitlines()]
    assert sorted(hits) == ["1:a { transition: opacity 200ms ease-in", '2:<meta content="maximum-scale=1">',
                            '3:<meta content="maximum-scale=1.0, width=device-width">'], hits
    # An image tag whose attributes sit on later lines shows all of them.
    (site / "index.html").write_text('<h1>A</h1>\n<img\n  src="a.png"\n  alt="A"\n  width="10"\n  height="10">\n'
                                     '<img src="b.png">\n', encoding="utf-8")
    (site / "DESIGN.md").write_text("Use <img with alt.\n", encoding="utf-8")
    images = block_rg("web-ui-basics", site)[3]
    assert all(attr in images.stdout for attr in ('alt="A"', 'width="10"', 'height="10"', '<img src="b.png">')), images
    assert "DESIGN.md" not in images.stdout, images
