import os


def remove(path):
    try:
        os.remove(path)
    except FileNotFoundError:
        pass
