"""Shared grouping and Markdown tables for the blocks a workflow chose or rejected."""


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
        for label, members in entries:
            if members:
                out.append(f"| {label} (sub-blocks) | |")
            for block in members or [label]:
                why = ledger[block][1].replace("|", "&#124;")
                out.append(f"| {'↳ ' if members else ''}{block} | {why} |")
        out.append("")
    return "\n".join(out)
