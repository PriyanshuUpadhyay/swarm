"""Shared grouping and Markdown tables for the blocks a workflow chose or rejected."""


NOT_NAMED_REASON = "not named by a domain line"


def ledger_groups(ledger, aliases):
    """[(status, [(label, [members])])]; alias members appear under their alias."""
    groups = []
    for status in ("Chosen", "Rejected"):
        names = [b for b, (s, _) in ledger.items() if s == status]
        entries = []
        for alias, members in aliases.items():
            here = [m for m in members if m in names]
            if here:
                entries.append((alias, here))
        grouped = {m for _, ms in entries for m in ms}
        entries += [(b, []) for b in names if b not in grouped]
        groups.append((status, entries))
    return groups


def ledger_markdown(ledger, aliases):
    out = []
    for status, entries in ledger_groups(ledger, aliases):
        out += [f"### {status}", "", "| Block | Why |", "|---|---|"]
        reasons = {}
        if status == "Rejected":
            for block, (state, why) in ledger.items():
                if state == status and why == NOT_NAMED_REASON:
                    reasons.setdefault(why, []).append(block)
        repeated = {why: names for why, names in reasons.items() if len(names) > 1}
        grouped = {name for names in repeated.values() for name in names}
        for label, members in entries:
            members = [block for block in members if block not in grouped]
            if label in grouped or label not in ledger and not members:
                continue
            if members:
                out.append(f"| {label} (sub-blocks) | |")
            for block in members or [label]:
                why = ledger[block][1].replace("|", "&#124;")
                out.append(f"| {'↳ ' if members else ''}{block} | {why} |")
        for why, names in repeated.items():
            out.append(f"| {', '.join(names)} | {why.replace('|', '&#124;')} |")
        out.append("")
    return "\n".join(out)
