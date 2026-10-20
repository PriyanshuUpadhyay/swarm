# Build execution and isolated verification worktrees.
import json, os, signal, subprocess, urllib.parse
from contextlib import contextmanager, nullcontext
from pathlib import Path


def verify_tree(head):
    """Keep range builds apart from the live checkout, at the reviewed commit."""
    from review_check import git, home
    verify = home() / "verify"
    if not verify.exists():
        git("worktree", "add", "--force", "--detach", str(verify), head)
    else:
        root = git("-C", str(verify), "rev-parse", "--show-toplevel", check=False).strip()
        if not root or Path(root).resolve() != verify.resolve():
            raise SystemExit(f"verify path is not its own git worktree: {verify}")
        git("-C", str(verify), "reset", "-q", "--hard")
        git("-C", str(verify), "clean", "-qfd")
        git("-C", str(verify), "checkout", "--detach", head)
    return verify



@contextmanager
def build_signals():
    def stop(signum, frame):
        raise SystemExit(128 + signum)

    previous = {signum: signal.getsignal(signum) for signum in (signal.SIGTERM, signal.SIGHUP)}
    try:
        for signum, handler in previous.items():
            # A signal ignored at entry stays ignored, so a nohup build survives a hangup.
            if handler != signal.SIG_IGN:
                signal.signal(signum, stop)
        yield
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)



def build(name, *opts):
    """Write build.json from CI or a command on the reviewed commit's verify worktree."""
    from review_check import BUILD_TIMEOUT, CLEANUP_SECONDS, build_signals, home, step_run, tree_digest, verify_tree
    if opts not in ((), ("--run",)):
        raise SystemExit("build accepts only --run; --force cannot override a running build")
    d = home() / name
    t = json.loads((d / "target.json").read_text(encoding="utf-8"))
    ci, head = t["ci"], t["head"]
    if not ci:
        raise SystemExit("the repo rules file has no `CI:` line, so this run has no build gate")
    cwd = Path.cwd()
    if opts[:1] == ("--run",):
        if not head:
            print("build: local target; proof is from the live tree")
        with (step_run.step_lock(home(), "verify", command="build --run") if head else nullcontext()), build_signals():
            if head:
                cwd = verify_tree(head)
            process = None
            try:
                try:
                    # Defer launch signals until process is set. The child inherits this blocked mask across exec,
                    # so preexec_fn resets it before exec; finally restores only the parent's mask.
                    previous_mask = signal.pthread_sigmask(signal.SIG_BLOCK, (signal.SIGTERM, signal.SIGHUP, signal.SIGINT))
                    try:
                        process = subprocess.Popen(["bash", "-c", ci["cmd"]], cwd=cwd, stdout=subprocess.PIPE,
                                                   stderr=subprocess.PIPE, text=True, start_new_session=True,
                                                   preexec_fn=lambda: signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask))
                    finally:
                        signal.pthread_sigmask(signal.SIG_SETMASK, previous_mask)
                    stdout, stderr = process.communicate(timeout=BUILD_TIMEOUT)
                    conclusion, tail = ("success" if process.returncode == 0 else "failure"), (stdout + stderr).splitlines()[-40:]
                except (subprocess.TimeoutExpired, KeyboardInterrupt, SystemExit) as error:
                    if process is None:
                        raise
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    try:
                        process.communicate(timeout=CLEANUP_SECONDS)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        # A child in another session may still hold the output pipes open.
                        process.stdout.close()
                        process.stderr.close()
                        try:
                            process.wait(timeout=CLEANUP_SECONDS)
                        except subprocess.TimeoutExpired:
                            pass
                    if not isinstance(error, subprocess.TimeoutExpired):
                        raise
                    conclusion, tail = "failure", [f"timed out after {BUILD_TIMEOUT} s"]
            finally:
                if process is not None:
                    if not process.stdout.closed:
                        process.stdout.close()
                    if not process.stderr.closed:
                        process.stderr.close()
        result = {"source": "command", "name": ci["cmd"], "conclusion": conclusion, "tail": tail}
    elif not head:
        raise SystemExit("uncommitted changes have no CI result; use `build <run> --run`")
    else:
        q = urllib.parse.quote(ci["name"])
        try:
            out = subprocess.run(["gh", "api", f"repos/{{owner}}/{{repo}}/commits/{head}/check-runs?check_name={q}"],
                                 cwd=cwd, capture_output=True, text=True, timeout=BUILD_TIMEOUT)
        except subprocess.TimeoutExpired:
            raise SystemExit(f"gh: timed out after {BUILD_TIMEOUT} s") from None
        except (FileNotFoundError, NotADirectoryError):
            raise SystemExit("gh: not found") from None
        except OSError as error:
            raise SystemExit(f"gh: {error.strerror or error}") from None
        if out.returncode != 0:
            raise SystemExit(f"gh: {out.stderr.strip()}")
        # skipped, cancelled, or running checks prove nothing; a re-run of the same head gets a higher id
        found = sorted((c for c in json.loads(out.stdout)["check_runs"] if c["status"] == "completed"
                        and c["conclusion"] in ("success", "failure", "timed_out")), key=lambda c: c["id"])
        if not found:
            raise SystemExit(f"no finished `{ci['name']}` check for {head[:7]} (not pushed, a draft PR, or still "
                             "running); wait, or use `build <run> --run`")
        last = found[-1]
        result = {"source": "check-run", "name": ci["name"],
                  "conclusion": "success" if last["conclusion"] == "success" else "failure", "tail": [last["html_url"]]}
    result |= {"head": head, "digest": None if head else tree_digest()}
    (d / "build.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"build: {result['conclusion']} ({result['source']} `{result['name']}`)")

