import logging


# A broad catch that re-raises, or that logs the traceback, keeps the error visible.
def run(job):
    try:
        job()
    except Exception as err:
        raise RuntimeError("job failed") from err


def serve(job):
    try:
        job()
    except Exception:
        logging.exception("job failed")
