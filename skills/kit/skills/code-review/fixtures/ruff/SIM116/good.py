PRICES = {"free": 0, "pro": 10, "team": 20}


def price(plan):
    return PRICES.get(plan, 99)
