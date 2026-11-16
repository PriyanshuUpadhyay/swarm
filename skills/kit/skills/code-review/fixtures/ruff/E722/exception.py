import os


# A bare except that re-raises only adds cleanup, so the error still goes up.
def remove(path, log):
    try:
        os.remove(path)
    except:
        log.append(path)
        raise
