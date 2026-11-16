def price(plan):
    if plan == "free":  # expect
        return 0
    elif plan == "pro":
        return 10
    elif plan == "team":
        return 20
    return 99
