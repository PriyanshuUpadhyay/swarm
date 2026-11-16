# Branches that call code, not return a literal, are not a lookup table.
def price(plan, bill):
    if plan == "free":
        return bill.free()
    elif plan == "pro":
        return bill.pro()
    elif plan == "team":
        return bill.team()
    return bill.custom()
