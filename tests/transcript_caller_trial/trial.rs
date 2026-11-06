//! Trial-only Root operations: guarded jobs, cancel, process state and close with a staged job.
//! A child module of `client`, so it shares the session. The old-worker entry never calls it.
//! Root never receives an uncommitted saved Begin/Part: the authority stages them privately, so
//! `start_hold` expects only Descriptor then Staged, and every cancel/close frame is strict.

use super::*;

impl Session {
    /// The tag a frame about the held job must carry.
    fn held_tag(f: &Value, held_id: u64, transport: u64) -> bool {
        f["tag"] == json!({"binding": binding(), "request_id": held_id, "transport_id": transport})
            && f.get("request_id").is_none()
    }

    /// Starts a genuine UntilCancelled guarded job. Expects Descriptor then Staged, nothing else.
    pub fn start_hold(&mut self, after_commit: &str, gate: &Path) -> Result<Held, String> {
        let obs = Obs::new();
        let guard =
            json!({"kind": "hold_before_commit", "gate_path": gate, "after_commit": after_commit});
        let (id, mut r) = self.build("read", json!({"start_byte": 0, "end_byte": crate::fixtures::END1, "end_policy": "complete_lines", "guard": guard}), &obs)?;
        r["work"] = json!({"kind": "until_cancelled"});
        self.send(&r, &obs)?;
        let (mut pid, mut transport) = (None, None);
        loop {
            let f = self.frame_now(&obs, "staged")?;
            if f["op"] == "fault" {
                return Err(format!("fault frame {f}"));
            }
            self.tag_check(&f, id, Kind::Saved, &mut transport)?;
            match f["op"].as_str() {
                Some("descriptor") if pid.is_none() => {
                    let same = self
                        .opened
                        .as_ref()
                        .is_some_and(|(d, i)| f["device_id"] == *d && f["inode_id"] == *i);
                    pid = f["pid"].as_u64().filter(|p| *p > 0 && same);
                    if pid.is_none() {
                        return Err(format!("descriptor identity or pid wrong: {f}"));
                    }
                }
                Some("staged") if pid.is_some() => {
                    return Ok(Held {
                        id,
                        transport_id: transport.expect("tag_check set it"),
                        pid: pid.expect("checked"),
                    });
                }
                _ => {
                    return Err(format!(
                        "unexpected frame before Staged (Root must not see a forwarded Begin/Part/Commit): {f}"
                    ));
                }
            }
        }
    }

    /// Cancels `held`: Fenced for that exact tag, then Cancelled with the exact fields and exit kind.
    /// A Begin, Part or Commit of the held job at any point is a published result and fails.
    /// The caller passes the one cancellation observation, shared with the later `ps` absence.
    pub fn cancel(&mut self, held: &Held, exit: Value, obs: &Obs) -> Result<(), String> {
        let (id, r) = self.build("cancel", json!({"target_request_id": held.id}), obs)?;
        self.send(&r, obs)?;
        let mut fenced = false;
        loop {
            let f = self.frame_now(obs, "cancelled")?;
            match f["op"].as_str() {
                Some("fenced") if !fenced && Self::held_tag(&f, held.id, held.transport_id) => {
                    fenced = true
                }
                Some("cancelled") if fenced => {
                    let fields = f["request_id"] == id
                        && f["binding"] == binding()
                        && f["target_request_id"] == held.id
                        && f["transport_id"] == held.transport_id;
                    let kind_ok = f["exit"]["kind"] == exit["kind"];
                    let value_ok = match exit["kind"].as_str() {
                        Some("code") => f["exit"] == exit,
                        _ => f["exit"]["signal"].is_u64(),
                    };
                    return if fields && kind_ok && value_ok {
                        Ok(())
                    } else {
                        Err(format!("cancelled fields or exit wrong: {f}"))
                    };
                }
                _ => {
                    return Err(format!(
                        "unexpected frame during cancel (no saved payload may appear): {f}"
                    ));
                }
            }
        }
    }

    /// Close (optionally with a staged job): Closed, then control stdin is dropped, both authority
    /// pipes reach EOF and the authority exits 0, all within the caller's one Close observation.
    pub fn close(&mut self, held: Option<&Held>, obs: &Obs) -> Result<(), String> {
        let (id, r) = self.build("close", json!({}), obs)?;
        self.send(&r, obs)?;
        let mut fenced = false;
        loop {
            let f = self.frame_now(obs, "closed")?;
            match f["op"].as_str() {
                Some("closed") if f["request_id"] == id && f["binding"] == binding() => break,
                Some("fenced")
                    if !fenced
                        && held.is_some_and(|h| Self::held_tag(&f, h.id, h.transport_id)) =>
                {
                    fenced = true
                }
                _ => return Err(format!("unexpected frame during close: {f}")),
            }
        }
        self.p.close_stdin();
        if let Some(f) = self.frame_by(obs)? {
            return Err(format!("frame after closed: {f}"));
        }
        self.p.drain_err(obs)?;
        let status = self.p.wait_exit(obs)?;
        if status.code() != Some(0) {
            return Err(format!("authority exit {status:?}"));
        }
        self.p.finish(false, obs)
    }

    /// `/bin/ps -o stat= -p PID` once, owned and bounded by `obs`. Live: exit 0, empty stderr,
    /// exactly one nonempty line. Absent: exit 1, empty stdout and empty stderr. On a timeout the
    /// ps child is killed and reaped in a separate cleanup observation, and the check fails.
    pub fn ps(&self, pid: u64, live: bool, obs: &Obs) -> Result<(), String> {
        let mut cmd = Command::new("/bin/ps");
        cmd.args(["-o", "stat=", "-p", &pid.to_string()])
            .env_clear()
            .current_dir(&self.root);
        let mut p = Proc::spawn("ps", cmd, false, obs)?;
        let mut run = || -> Result<(String, String, Option<i32>), String> {
            let mut out = Vec::new();
            while let Some(chunk) = p.next_out(obs)? {
                out.extend(chunk);
            }
            let err = p.drain_err(obs)?;
            let status = p.wait_exit(obs)?;
            Ok((
                String::from_utf8_lossy(&out).into_owned(),
                String::from_utf8_lossy(&err).into_owned(),
                status.code(),
            ))
        };
        let (so, se, code) = match run() {
            Ok(v) => v,
            Err(e) => {
                let cleanup = p.finish(true, &Obs::new());
                return Err(format!("ps incomplete: {e}; cleanup {cleanup:?}"));
            }
        };
        p.finish(false, obs)?;
        let ok = if live {
            code == Some(0) && se.is_empty() && so.lines().count() == 1 && !so.trim().is_empty()
        } else {
            code == Some(1) && so.is_empty() && se.is_empty()
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "ps pid {pid} live={live}: code {code:?} stdout {so:?} stderr {se:?}"
            ))
        }
    }
}
