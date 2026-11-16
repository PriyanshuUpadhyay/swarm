import json


def load(path):
    try:
        return json.loads(open(path).read())
    except Exception:  # expect
        return None


def stop(task):
    try:
        task.cancel()
    except (ValueError, BaseException):  # expect
        pass
