# Review rows, answers, and the final verdict gate.
import hashlib, json, re
from pathlib import Path


def review_files(d, seats):
    if not seats:
        return sorted(d.glob("02-review*.md"))
    files = [d / "02-review.md", *(d / f"02-review-{seat}.md" for seat in seats)]
    return [f for f in files if f.exists()]



def rows(d, seats=None):
    from review_check import review_files
    out = []
    for f in review_files(d, seats or {}):
        if not f.exists():
            continue
        for line in f.read_text(encoding="utf-8").split("\n"):
            if re.match(r"\|\s*u\d+\s*\|", line):
                cells = [c.strip().replace("\\|", "|") for c in re.split(r"(?<!\\)\|", line)[1:-1]]
                out.append((f.name, cells))
    return out



def answers(path):
    out = []
    for line in path.read_text(encoding="utf-8").split("\n") if path.exists() else []:
        if m := re.match(r"^\s*- (.+?) `(.*)$", line):
            file, text = m.groups()
            alternatives = [(file, text[:split.start()], text[split.end():])
                            for split in re.finditer(r"`: ?", text) if split.start()]
            if alternatives:
                out.append(alternatives)
    return out



def verdict(name):
    from review_check import GENERIC, KINDS, answers, home, parse_diff, revision, rows, step_run, tree_digest
    d = home() / name
    units = {u["id"]: u for u in json.loads((d / "units.json").read_text(encoding="utf-8"))}
    diff = parse_diff((d / "diff.patch").read_text(encoding="utf-8"))
    rules = re.search(r"^Rules: (.*)$", (d / "01-units.md").read_text(encoding="utf-8"), re.M).group(1)
    checklist = json.loads((d / "checklist.json").read_text(encoding="utf-8"))
    seats = checklist.pop("seats", {})
    t = json.loads((d / "target.json").read_text(encoding="utf-8")) if (d / "target.json").exists() else {"ci": None}
    problems, seen, kept, done = [], set(), [], {uid: set() for uid in units}
    snapshot = {"open": [], "checks": [], "certain": True}
    symbols = [(u["file"], u["symbol"]) for u in units.values()]
    if len(set(symbols)) != len(symbols):
        snapshot["certain"] = False
    replied = answers(d / "answers.md")
    past, history = answers(d / "answers-before.md"), []
    closed = [0] * len(replied)
    selected = [alternatives[0] for alternatives in replied]
    built = "none"
    if t["ci"]:  # the build must pass even with no tool IDs, because a missing import breaks no rule ID
        b = json.loads((d / "build.json").read_text(encoding="utf-8")) if (d / "build.json").exists() else None
        if b is None:
            problems.append("build: no result; run `build <run>`, or `build <run> --run` for an unpushed head")
        elif b["head"] != t["head"] or (t["head"] is None and b["digest"] != tree_digest()):
            problems.append("build: the result is for other code; run `build` again")
        else:
            built = "pass" if b["conclusion"] == "success" else "fail"
            for uid in units:
                done[uid] |= set(checklist[uid].get("tool", []))
            if built == "fail":
                kept.append(("build", f"build {b['name']}", "", "fix", f"build `{b['name']}` failed at "
                             f"{(t['head'] or 'working tree')[:7]}", b["tail"][-1] if b["tail"] else "no output"))
    for src, cells in rows(d, seats):
        if len(cells) != 7:
            problems.append(f"{src}: a row has {len(cells)} cells, not 7: {' | '.join(cells)}")
            continue
        uid, rule, loc, quote, kind, problem, proof = cells
        quote, kind = quote.strip("`"), kind.lower()
        ids = {r.strip().strip("`") for r in rule.split(",")} - {"-", ""}
        u = units.get(uid)
        if u is None or kind not in KINDS or not quote:
            problems.append(f"{uid}: unknown unit, kind not in {'/'.join(KINDS)}, or no quote")
            continue
        if kind in ("fix", "ask", "note") and len(ids) > 1:
            problems.append(f"{uid}: a {kind} row names one rule, or `-` for a finding that no rule owns")
            continue
        unknown = ids - set(checklist[uid]["rules"]) - set(checklist[uid].get("tool", []))
        for rule_id in sorted(unknown):
            problems.append(f"{uid}: {rule_id} is not in this unit's checklist")
        ids -= unknown
        if ids & set(checklist[uid].get("tool", [])):
            problems.append(f"{uid}: {', '.join(sorted(ids & set(checklist[uid]['tool'])))} belongs to the build gate, not a seat")
            continue
        if seats and src == "02-review.md":
            owners = checklist[uid].get("owners", {})
            owned = {r for r in ids if r in owners}
            if owned:
                for seat in sorted({owners[r] for r in owned}):
                    names = ', '.join(sorted(r for r in owned if owners[r] == seat))
                    problems.append(f"{uid}: chair row names {names} owned by seat {seat}")
                continue
        elif seats:
            seat = src.removeprefix("02-review-").removesuffix(".md")
            wrong = {r for r in ids if checklist[uid].get("owners", {}).get(r) != seat}
            if wrong or uid not in seats[seat]["units"]:
                problems.append(f"seat {seat}: {uid}: row includes checks owned by another seat: {', '.join(sorted(wrong))}")
                continue
        if proof.lower().strip(". ") in GENERIC:
            problems.append(f"{uid} {rule}: a {kind} needs a proof that names the case, the input, or why the rule cannot apply")
            continue
        path = loc.split(":")[0].strip("`")
        if kind in ("pass", "n/a"):  # a quote from the unit, or from code the diff removed in that file
            src = d / "head" / u["file"]
            head = src.read_text(encoding="utf-8").splitlines() if src.exists() else []
            scope = head[max(u["range"][0] - 1, 0):u["range"][1]] + diff[u["file"]]["removed"]
        else:  # the quote must be at the line the row names, or in code the diff removed from that file
            at = re.search(r":(\d+)(?:-(\d+))?", loc)
            new = diff.get(path, {}).get("new", {})
            new_lines = [new[n] for n in range(int(at.group(1)), int(at.group(2) or at.group(1)) + 1) if n in new] if at else []
            matches = [(i, match) for i, alternatives in enumerate(replied)
                       if (match := next((alt for alt in alternatives if alt[0] == path
                                          and any(alt[1] == source.strip() for source in new_lines)
                                          and alt[2].strip().lower().startswith("limit:") == (kind == "fix")), None))] if kind in ("ask", "fix") else []
            scope = new_lines + diff.get(path, {}).get("removed", [])
        if not any(quote in line for line in scope):
            where = "in the unit" if kind in ("pass", "n/a") else f"at {loc.strip('`')} or in code removed from {path}"
            problems.append(f"{uid}: quote not found {where}: {quote!r}")
            continue
        seen.add(uid)
        done[uid] |= ids
        if kind in ("fix", "ask", "note"):
            problem = f"{rule}: {problem}" if ids else problem
            if kind == "ask" and not matches:
                for alternatives in past:
                    match = next((alt for alt in alternatives if alt[0] == path
                                  and any(alt[1] == source.strip() for source in new_lines)), None)
                    if match:
                        file, line, answer = match
                        history.append(f"History: {file} `{line}`: {answer}")
            if kind in ("ask", "fix") and matches:
                normalized = []
                for i, match in matches:
                    closed[i] += 1
                    selected[i] = match
                    text = match[2].strip()
                    normalized.append((text, text.lower().startswith("fix:")))
                answer, is_fix = next((item for item in normalized if item[1]), normalized[0])
                kind = "limit" if kind == "fix" else "fix" if is_fix else "answered"
                if kind == "fix":
                    problem = answer
                elif kind == "limit":
                    proof = answer[6:].strip()
                else:
                    proof = answer
            kept.append((uid, loc, quote, kind, problem, proof))
        evidence = sorted({line.strip() for line in scope if quote in line})
        if len(evidence) != 1 or u["symbol"].startswith(("hunk ", "deleted ")):
            snapshot["certain"] = False
        for rule_id in sorted(ids or {"-"}):
            identity = [u["file"], u["symbol"], rule_id, evidence]
            if rule_id == "-":
                identity.append(problem.strip())
            snapshot["checks"].append([*identity, kind])
            if kind in ("fix", "ask"):
                snapshot["open"].append(identity)
    for alternatives, match, n in zip(replied, selected, closed):
        path, quote, answer = match if n else min(alternatives, key=lambda alt: len(alt[2]))
        text = answer.strip()
        body = re.sub(r"^(?:fix|limit):\s*", "", text, flags=re.I)
        if body.lower().strip(". ") in GENERIC:
            problems.append(f"answers.md {path} `{quote}`: an answer needs a record the user approved or a failing test")
    missing = {uid: [r for r in checklist[uid]["rules"] if r not in done[uid]] for uid in units}
    if seats:
        for seat, plan in seats.items():
            for uid in plan["units"]:
                gaps = [r for r in missing[uid] if checklist[uid]["owners"][r] == seat]
                if gaps:
                    problems.append(f"seat {seat}: {uid}: no result for {', '.join(gaps)}")
    else:
        problems += [f"{uid}: no result for {', '.join(m)}" for uid, m in missing.items() if m]
    problems += [f"{uid}: no row ({u['file']} {u['symbol']})" for uid, u in units.items() if uid not in seen]
    checks = sum(len(c["rules"]) + len(c.get("tool", [])) for c in checklist.values())
    checked = sum(len(done[uid] & set(c["rules"] + c.get("tool", []))) for uid, c in checklist.items())

    count = {k: sum(1 for r in kept if r[3] == k) for k in ("fix", "ask", "note", "answered", "limit")}
    if problems:
        status, word = "blocked gate failed", "INCOMPLETE"
    else:
        status = "done"
        word = "REQUEST CHANGES" if count["fix"] else "NEEDS DISCUSSION" if count["ask"] else "APPROVE"
    line3 = (f"Verdict: {word} ({len(seen)} of {len(units)} units, {checked} of {checks} checks; {count['fix']} fix, {count['ask']} ask, "
             f"{count['note']} note, {count['answered']} answered, {count['limit']} limit; rules: {'none' if rules == 'none' else Path(rules).name}; build: {built})")
    uses = ", ".join(f"{f.stem}@{revision(f)}" for f in [d / "01-units.md", *sorted(d.glob("02-review*.md")),
                                                          *[f for f in [d / "build.json", d / "answers.md"] if f.exists()]])
    body = [line3, "", *(f"- {p}" for p in problems),
            *(f"- {k} `{loc}` {problem} ({'answer' if k == 'answered' else 'limit' if k == 'limit' else 'proof'}: {proof})"
              for _, loc, _, k, problem, proof in kept),
            *(f"Answered: {path} `{quote}` closes {n} {'fixes' if answer.strip().lower().startswith('limit:') else 'asks'}"
              for (path, quote, answer), n in zip(selected, closed)), *history]
    rest = f"Uses: {uses}\n" + "\n".join(body) + "\n"
    rev = f" {hashlib.sha1(rest.encode()).hexdigest()[:12]}" if status == "done" else ""
    (d / "03-verdict.md").write_text(f"Status: {status}{rev}\n{rest}", encoding="utf-8")
    snapshot["complete"] = not problems and built != "fail"
    snapshot["checks"] = sorted(snapshot["checks"])
    snapshot["open"] = sorted(snapshot["open"])
    snapshot["verdict"] = revision(d / "03-verdict.md")
    step_run.write_step(d / "snapshot.json", json.dumps(snapshot, indent=2) + "\n")
    print(*(re.sub(r"[\x00-\x08\x0b-\x1f\x7f-\x9f]", "", line) for line in body if line), sep="\n")
    return 1 if problems else 0

