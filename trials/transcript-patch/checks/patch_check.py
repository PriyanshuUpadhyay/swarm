"""Checker-owned patch observation runner v7 (unit09 PATCH-V1). Rules, verdicts and exit codes are in README.md. v1-v6 are evidence; v2 is UNSAFE.
usage: python3.12 -B -I patch_check.py --exe BINARY --root NEW_DIR --owner TEXT [--candidate markdown] [--runs 2] [--expect-exe-sha256 HEX] | --selftest
Runs the EXISTING envelope  EXE CANDIDATE task INPUTS --composed FILE --out DIR  per group (legit, over, inf), --runs times each. FREEZE hashes are checked
and OWNER.json is fsynced before the first spawn. One run is one 900 ms envelope that ends with the final fence (the last step; it only demotes to
INCOMPLETE). A group is signalled only while its leader is unreaped; after the reap nothing is signalled.
v7 OUTPUT SAFETY: --out is opened with O_NOFOLLOW and surveyed by lstat BEFORE any read; survey closes its fd on any error. A missing --out or required
file is INCOMPLETE. A symlink or non-directory --out, a non-regular required file, or any extra entry is a layout RED-MISMATCH and is never opened; the
first two skip all domain reads. Regular files are read through O_NOFOLLOW|O_NONBLOCK plus fstat (leader reaped, group absent).
Numbers compare as binary64 (1 == 1.0); bool, null, absent key, string units and array order stay exact."""
import errno, hashlib, json, math, os, select, signal, stat, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
# NATIVE existing adapter layout (not JS domain values): file -> required value; None = compared against patch_expected.json instead.
LAYOUT = {"patch.json": None, "comment.json": None, "markdown.json": {}, "find.json": {}, "c1-composed.json": [], "raw-composed.json": []}
BUDGET = 0.9  # also the length of the single shared cleanup deadline
GROUPS = ("legit", "over", "inf")
FILES = ("patch_inputs.json", "patch_expected.json", "patch_check.py", "make_expected.py", "README.md")
sha = lambda b: hashlib.sha256(b).hexdigest()
utc = lambda: time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def _no_constant(tok):
    raise ValueError("JSON token " + tok + " is not allowed")


def read_regular(name, dir_fd=None):
    """Bytes of a REGULAR file only: O_NOFOLLOW|O_NONBLOCK open, then fstat; a link, FIFO or directory raises before any read."""
    fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, dir_fd=dir_fd)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise OSError("not a regular file")
        parts = []
        while True:
            b = os.read(fd, 1 << 16)
            if not b:
                return b"".join(parts)
            parts.append(b)
    finally:
        os.close(fd)


def load_js(name, dir_fd=None):
    return json.loads(read_regular(name, dir_fd).decode("utf-8"), parse_int=float, parse_float=float, parse_constant=_no_constant)


def survey(outdir):
    """Entry types of --out by lstat; opens no entry. -> (dir fd or None, {name: is_regular}, problem or None)."""
    try:
        dfd = os.open(outdir, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    except FileNotFoundError:
        return None, {}, "missing"
    except OSError as e:
        if e.errno not in (errno.ELOOP, errno.ENOTDIR):
            raise
        return None, {}, f"out is not a plain directory (errno {e.errno})"
    try:
        return dfd, {n: stat.S_ISREG(os.lstat(n, dir_fd=dfd).st_mode) for n in os.listdir(dfd)}, None
    except BaseException:
        os.close(dfd)
        raise


def want_js(v):
    if isinstance(v, bool) or v is None or isinstance(v, str):
        return v
    if isinstance(v, (int, float)):
        return float(v)
    return [want_js(x) for x in v] if isinstance(v, list) else {k: want_js(x) for k, x in v.items()}


def same(a, b, path="$", out=None):
    out = [] if out is None else out
    if isinstance(a, bool) or isinstance(b, bool):
        ok = isinstance(a, bool) and isinstance(b, bool) and a == b
    elif isinstance(a, float) or isinstance(b, float):
        ok = isinstance(a, float) and isinstance(b, float) and (a == b or (math.isnan(a) and math.isnan(b)))
    elif isinstance(a, dict) and isinstance(b, dict):
        if set(a) != set(b):
            out.append(f"{path}: keys native-only {sorted(set(a) - set(b))} original-only {sorted(set(b) - set(a))}")
            return False
        return all([same(a[k], b[k], f"{path}.{k}", out) for k in a])
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            out.append(f"{path}: length native {len(a)} != original {len(b)}")
            return False
        return all([same(x, y, f"{path}[{i}]", out) for i, (x, y) in enumerate(zip(a, b))])
    else:
        ok = type(a) is type(b) and a == b
    if not ok:
        out.append(f"{path}: native {json.dumps(a)[:120]} != original {json.dumps(b)[:120]}")
    return ok


def selftest():
    cases = [(1.0, 1.0, True), (1e20, float("100000000000000000000"), True), (True, 1.0, False), (None, 0.0, False), ("a", "b", False), (None, None, True),
             ({"a": 1.0}, {"a": 1.0, "b": None}, False), ({"a": 1.0, "x": 2.0}, {"a": 1.0}, False), ([1.0, 2.0], [2.0, 1.0], False), (float("inf"), None, False)]
    bad = [c for c in cases if same(c[0], c[1]) != c[2]]
    p = json.loads('{"a": 1, "b": 1.0, "d": ' + "9" * 400 + "}", parse_int=float, parse_float=float)
    ok = not bad and p["a"] == p["b"] == 1.0 and p["d"] == float("inf")
    try:
        json.loads("[Infinity]", parse_constant=_no_constant)
        ok = False
    except ValueError:
        pass
    d = tempfile.mkdtemp()  # a regular file, symlinks and a FIFO in an own dir; no child, no wait
    dfd = None
    try:
        open(f"{d}/reg.json", "w").write("{}")
        os.symlink(f"{d}/reg.json", f"{d}/link.json")
        os.mkfifo(f"{d}/fifo.json")
        dfd, ents, prob = survey(d)
        ok = ok and prob is None and ents == {"reg.json": True, "link.json": False, "fifo.json": False} and read_regular("reg.json", dfd) == b"{}"
        for n in ("link.json", "fifo.json"):
            try:
                read_regular(n, dfd)
                ok = False
            except OSError:
                pass
        os.close(dfd)
        dfd = None
        os.symlink(d, f"{d}/dirlink")
        for p, kind in ((f"{d}/dirlink", "plain"), (f"{d}/reg.json", "plain"), (f"{d}/nope", "missing")):
            r = survey(p)
            ok = ok and r[0] is None and r[1] == {} and kind in r[2]
        free = os.open(os.devnull, os.O_RDONLY)
        os.close(free)
        real, os.listdir = os.listdir, lambda *a: 1 / 0
        try:
            survey(d)
            ok = False
        except ZeroDivisionError:
            pass
        finally:
            os.listdir = real
        again = os.open(os.devnull, os.O_RDONLY)
        os.close(again)
        ok = ok and again == free  # a leaked fd would make the next free one higher
    finally:
        if dfd is not None:
            os.close(dfd)
        for n in os.listdir(d):
            os.unlink(f"{d}/{n}")
        os.rmdir(d)
    print("selftest", "ok" if ok else f"FAILED {bad}")
    return 0 if ok else 1


def probe(pgid):
    """Signal-0 probe of the run's own group id (presence only, no ownership)."""
    try:
        os.killpg(pgid, 0)
        return "present"
    except ProcessLookupError:
        return "absent"
    except PermissionError:
        return "eperm"
    except OSError as e:
        return f"probe errno {e.errno}"


def observe(exe, exe_info, candidate, root, env, name, group, inputs, expected):
    ids_p, ids_c = [c["id"] for c in inputs["patch"]], [c["id"] for c in inputs["comment"]]
    inp, outdir = f"{root}/{name}.inputs.json", f"{root}/{name}-out"
    with open(inp, "x", encoding="utf-8") as f:
        json.dump(inputs, f)
    argv = [exe, candidate, "task", inp, "--composed", f"{root}/composed.json", "--out", outdir]
    why, note, diffs, green = [], {}, [], []
    p = fo = fe = kq = None
    pgid = rc = None
    persisted = note_exit = timed_out = reaped = False
    status, err, cs = None, "", {}
    t0 = time.monotonic()  # the envelope opens here, before the spawn
    deadline = t0 + BUDGET
    ident = {"name": name, "group": group, "argv": argv, "exe": exe_info, "spawn_utc": utc()}
    try:
        try:
            fo, fe = open(f"{root}/{name}.stdout", "xb"), open(f"{root}/{name}.stderr", "xb")
            p = subprocess.Popen(argv, cwd=f"{root}/work", env=env, stdin=subprocess.DEVNULL, stdout=fo, stderr=fe, start_new_session=True)
            pgid = p.pid
            ident.update(pid=p.pid, pgid=pgid)
            try:  # watch registered immediately after the spawn, before the identity fsync
                kq = select.kqueue()
                kq.control([select.kevent(p.pid, filter=select.KQ_FILTER_PROC, flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT, fflags=select.KQ_NOTE_EXIT)], 0, 0)
            except OSError as e:
                why.append(f"NOTE_EXIT registration failed: errno {e.errno}")
            with open(f"{root}/{name}.identity.json", "x") as f:
                json.dump(ident, f)
                f.flush()
                os.fsync(f.fileno())
            persisted = True
            if kq is not None:  # the wait starts only now
                evs = kq.control(None, 1, max(deadline - time.monotonic(), 0))
                note_exit = any(e.fflags & select.KQ_NOTE_EXIT for e in evs)
                timed_out = not evs
        except Exception as e:
            why.append(f"harness error before exit: {type(e).__name__}: {str(e)[:100]}")
        if not persisted:
            why.append("identity not persisted")
        if timed_out:
            why.append("deadline: no exit within 900 ms")
        elif p is not None and not note_exit:
            why.append("NOTE_EXIT not observed")
        def crem():  # one shared cleanup deadline, at most 900 ms
            cs.setdefault("dl", time.monotonic() + BUDGET)
            return max(cs["dl"] - time.monotonic(), 0)

        def sigkill(tag):  # the only signal path; refuses once the leader is reaped
            if reaped or p is None or p.returncode is not None:
                raise RuntimeError("refusing to signal a group after the leader was reaped")
            try:
                os.killpg(pgid, signal.SIGKILL)
                note.setdefault("signals", []).append(tag)
            except (ProcessLookupError, PermissionError) as e:
                note.setdefault("signal_errors", []).append(f"{tag}: errno {e.errno}")

        if p is not None and not reaped:  # the leader is still unreaped here: inspect and, if needed, clean BEFORE the reap
            if note_exit:
                pre = probe(pgid)
                note["group_at_exit_before_reap"] = pre
                if pre == "present":  # a live member besides the zombie leader
                    why.append("live group member at leader exit")
                    sigkill("pre-reap, live member at exit")
                elif pre != "eperm":  # eperm = only the zombie leader is left
                    why.append(f"unexpected group probe before reap: {pre}")
            else:
                sigkill("pre-reap, no exit seen")
                note["cleanup_note_exit"] = None  # UNKNOWN unless the post-signal event arrives
                if kq is not None:
                    try:
                        note["cleanup_note_exit"] = bool(kq.control(None, 1, crem()))
                    except OSError:
                        pass
            try:
                rc = p.wait(timeout=crem())  # shares the one cleanup deadline; no renewed wait
                reaped = True
            except subprocess.TimeoutExpired:
                why.append("leader not reaped within the cleanup deadline")
        note["group_after_reap"] = "UNKNOWN"
        if reaped:
            g = probe(pgid)  # one probe, no signal, no retry
            note["group_after_reap"] = g
            if g != "absent":
                why.append(f"group {g} after reap: UNKNOWN, retained, not signalled")
        else:
            why.append("leader never reaped: group state UNKNOWN")
        dfd = None
        if reaped and rc == 0 and note_exit and not why:
            try:
                dfd, ents, prob = survey(outdir)  # types first; nothing is opened yet
                absent = sorted(set(LAYOUT) - set(ents))
                unsafe = sorted(n for n in LAYOUT if n in ents and not ents[n])
                extras = sorted(set(ents) - set(LAYOUT))
                if prob == "missing":
                    why.append("required output missing: out directory")
                elif prob:  # symlink or non-directory out: layout RED, nothing read
                    diffs.append(f"layout: {prob}")
                elif absent:
                    why.append(f"required output missing: {absent}")
                elif unsafe or extras:
                    diffs.append(f"layout: extras (never opened) {extras} non-regular required (never opened) {unsafe}")
                if prob is None and not absent and not unsafe:  # only regular required files are read
                    for n, want_l in LAYOUT.items():
                        dl = []
                        if want_l is not None and same(load_js(n, dfd), want_l, f"layout.{n}", dl):
                            green.append(f"layout.{n}")
                        elif want_l is not None:
                            diffs.extend(dl)
                    pj, cj = load_js("patch.json", dfd), load_js("comment.json", dfd)
                    whole = {"patch": (pj, {i: expected["patch"][i] for i in ids_p}), "comment": (cj, {i: expected["comment"][i] for i in ids_c})}
                    for kind, (got, want) in whole.items():
                        d = []
                        if not isinstance(got, dict):
                            d.append(f"{kind}.json is not an object")
                        else:
                            if set(got) != set(want):
                                d.append(f"{kind}.json ids native-only {sorted(set(got) - set(want))} original-only {sorted(set(want) - set(got))}")
                            for i in want:
                                ds = []
                                if i in got and same(got[i], want_js(want[i]), f"{kind}.{i}", ds):
                                    green.append(f"{kind}.{i}")
                                else:
                                    d.extend(ds)
                        diffs.extend(d)
            except Exception as e:
                why.append(f"outputs unreadable with exit 0: {type(e).__name__}: {str(e)[:100]}")
            finally:
                if dfd is not None:
                    os.close(dfd)
        # every read the verdict uses happens here, inside the envelope, before close and the fence
        try:
            err = read_regular(f"{root}/{name}.stderr").decode("utf-8", "replace") if os.path.exists(f"{root}/{name}.stderr") else ""
        except OSError as e:
            why.append(f"stderr file unreadable: {e}")
        status = "INCOMPLETE" if why else "RED-NATIVE-FAILURE" if rc != 0 else "RED-MISMATCH" if diffs else "GREEN"
    finally:
        for h in (kq, fo, fe):
            try:
                if h is not None:
                    h.close()
            except OSError:
                pass
        if p is not None and not reaped and p.returncode is None:
            try:  # leader still unreaped, so the group id is still pinned
                os.killpg(pgid, signal.SIGKILL)
                note.setdefault("signals", []).append("finally, leader unreaped")
            except (ProcessLookupError, PermissionError):
                pass
            try:
                rc = p.wait(timeout=max(cs.get("dl", time.monotonic() + BUDGET) - time.monotonic(), 0))
                reaped = True
            except subprocess.TimeoutExpired:
                pass
            why.append("cleanup ran in finally after an error")
            status = "INCOMPLETE"
    elapsed = time.monotonic() - t0  # FINAL FENCE: last step that can change acceptance; it can only demote
    if status is None or elapsed > BUDGET:
        why.append(f"over 900 ms: {elapsed:.3f}s" if elapsed > BUDGET else "no classification reached")
        status = "INCOMPLETE"
    rec = {**ident, "rc": rc, "timed_out": timed_out, "note_exit_seen": note_exit, "identity_persisted": persisted, "reaped": reaped, **note, "elapsed_s": round(elapsed, 4),
           "status": status, "incomplete_reasons": why, "green_count": len(green), "differences": diffs, "stderr_tail": err[-300:], "end_utc": utc()}
    with open(f"{root}/receipts.jsonl", "a") as f:
        f.write(json.dumps(rec) + "\n")
    tail = err.strip().splitlines()
    print(f"{name}: {status} rc={rc} {rec['elapsed_s']}s green={len(green)} diffs={len(diffs)}" + (f" why={why}" if why else "") + (f" | {tail[-2][:110]}" if status == "RED-NATIVE-FAILURE" and len(tail) > 1 else ""))
    return status


def main(argv):
    if "--selftest" in argv:
        return selftest()
    arg = lambda k, d=None: argv[argv.index(k) + 1] if k in argv and argv.index(k) + 1 < len(argv) else d
    exe, root, owner, candidate, runs = arg("--exe"), arg("--root"), arg("--owner"), arg("--candidate", "markdown"), int(arg("--runs", "2"))
    try:
        freeze = {}
        for line in open(f"{HERE}/FREEZE.sha256").read().splitlines():
            h, f = line.split("  ", 1)
            freeze[f] = h
        if set(freeze) != set(FILES):
            print(f"SETUP: FREEZE.sha256 names {sorted(freeze)}, expected {sorted(FILES)}")
            return 3
        for f in FILES:
            if sha(open(f"{HERE}/{f}", "rb").read()) != freeze[f]:
                print(f"SETUP: {f} differs from FREEZE.sha256")
                return 3
        if not exe or not root or not owner or not os.path.isfile(exe) or not os.access(exe, os.X_OK) or os.path.exists(root):
            print("SETUP: need --exe (executable file), --root (must not exist), --owner TEXT")
            return 3
        exe = os.path.abspath(exe)
        exe_sha = sha(open(exe, "rb").read())
        if arg("--expect-exe-sha256") and arg("--expect-exe-sha256") != exe_sha:
            print(f"SETUP: exe sha256 {exe_sha} differs from --expect-exe-sha256")
            return 3
        inputs = json.load(open(f"{HERE}/patch_inputs.json"))["groups"]
        expected = json.load(open(f"{HERE}/patch_expected.json"))
    except (OSError, KeyError, ValueError) as e:
        print(f"SETUP: {type(e).__name__}: {e}")
        return 3
    root = os.path.abspath(root)
    os.mkdir(root)
    with open(f"{root}/OWNER.json", "x") as f:  # owner record before any dispatch
        json.dump({"owner": owner, "runner_pid": os.getpid(), "root": root, "created_utc": utc(), "argv": argv, "freeze_verified": freeze, "exe": exe, "exe_sha256": exe_sha}, f)
        f.flush()
        os.fsync(f.fileno())
    for d in ("work", "home", "tmp", "xdg"):
        os.mkdir(f"{root}/{d}")
    with open(f"{root}/composed.json", "x") as f:
        f.write("[]")
    env = {"HOME": f"{root}/home", "TMPDIR": f"{root}/tmp", "XDG_CONFIG_HOME": f"{root}/xdg", "XDG_CACHE_HOME": f"{root}/xdg", "XDG_STATE_HOME": f"{root}/xdg", "XDG_DATA_HOME": f"{root}/xdg", "PATH": "/usr/bin:/bin"}
    info = {"path": exe, "sha256": exe_sha, "bytes": os.path.getsize(exe)}
    res = {f"{g}-{n}": observe(exe, info, candidate, root, env, f"{g}-{n}", g, inputs[g], expected) for g in GROUPS for n in range(1, runs + 1)}
    vals = set(res.values())
    verdict = "INCOMPLETE" if "INCOMPLETE" in vals else "RED" if any(v.startswith("RED") for v in vals) else "GREEN"
    print("VERDICT", verdict, json.dumps(res))
    return {"GREEN": 0, "RED": 1, "INCOMPLETE": 2}[verdict]


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
