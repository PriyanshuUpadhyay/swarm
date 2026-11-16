import json


def load(path):
    try:
        return json.loads(open(path).read())
    except (OSError, ValueError):
        return None
