// Provider process reads and Codex app-server transport.
use super::*;

pub fn read_json(
    executable: &str,
    args: &[&str],
    env: &BTreeMap<String, String>,
    deadline: Instant,
) -> Result<Value, NativeReadError> {
    if Instant::now() >= deadline {
        return Err("provider read timed out".into());
    }
    let mut command = Command::new(executable);
    let claude_auth = args == ["auth", "status"];
    if claude_auth {
        // Native status must not inherit a different profile from the caller shell.
        for key in Provider::Claude.account_env_keys() {
            command.env_remove(key);
        }
    }
    let child = command
        .args(args)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| NativeReadError::CliUnavailable)?;
    let (response, status) = read_stdout(child, u64::MAX, deadline);
    if response.is_err() || status.is_none() {
        return Err("provider read timed out".into());
    }
    if !status.is_some_and(|status| status.success() || (claude_auth && status.code() == Some(1))) {
        return Err("provider read is unavailable".into());
    }
    let bytes = response
        .map_err(|_| "provider read timed out")?
        .map_err(|_| "provider read failed")?;
    serde_json::from_slice(&bytes).map_err(|_| "provider response JSON is invalid".into())
}

/// Reads up to `limit` bytes of the child's stdout and waits for its exit, both before `deadline`.
/// The child is always killed and reaped. A status of `None` means it did not exit in time.
pub(crate) fn read_stdout(
    mut child: Child,
    limit: u64,
    deadline: Instant,
) -> (
    Result<std::io::Result<Vec<u8>>, RecvTimeoutError>,
    Option<ExitStatus>,
) {
    let (sender, receiver) = channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(limit).read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        });
    } else {
        let _ = sender.send(Err(std::io::Error::other("child has no stdout pipe")));
    }
    let response = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let mut status = child.try_wait().ok().flatten();
    while response.is_ok() && status.is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        status = child.try_wait().ok().flatten();
    }
    let _ = child.kill();
    let _ = child.wait();
    (response, status)
}

pub struct AppServer {
    child: Child,
    responses: Receiver<Result<Value, String>>,
    deadline: Instant,
}

impl AppServer {
    pub fn start(
        env: &BTreeMap<String, String>,
        deadline: Instant,
    ) -> Result<Self, NativeReadError> {
        if Instant::now() >= deadline {
            return Err("Codex read timed out".into());
        }
        let executable = std::env::var("SWARM_CODEX_CMD").unwrap_or_else(|_| "codex".into());
        let mut child = Command::new(executable)
            .args(["app-server", "--listen", "stdio://"])
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| NativeReadError::CliUnavailable)?;
        let stdout = child.stdout.take().ok_or("Codex stdout is unavailable")?;
        let (sender, responses) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let value = line
                    .map_err(|_| "Codex response read failed".into())
                    .and_then(|line| {
                        serde_json::from_str(&line)
                            .map_err(|_| "Codex response JSON is invalid".into())
                    });
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child,
            responses,
            deadline,
        };
        server.request(
            1,
            "initialize",
            json!({"clientInfo":{"name":"swarm","title":"Swarm","version":env!("CARGO_PKG_VERSION")}}),
        )?;
        server.send(json!({"method":"initialized","params":{}}))?;
        Ok(server)
    }

    fn send(&mut self, value: Value) -> Result<(), NativeReadError> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or("Codex stdin is unavailable")?;
        writeln!(stdin, "{value}")
            .and_then(|_| stdin.flush())
            .map_err(|_| "Codex request write failed".into())
    }

    pub fn request(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, NativeReadError> {
        if Instant::now() >= self.deadline {
            return Err("Codex read timed out".into());
        }
        self.send(json!({"id":id,"method":method,"params":params}))?;
        loop {
            if Instant::now() >= self.deadline {
                return Err("Codex read timed out".into());
            }
            let value = self
                .responses
                .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "Codex read timed out")??;
            if value["id"] != id {
                continue;
            }
            if value.get("error").is_some() {
                return Err("Codex request failed".into());
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "Codex response has no result".into());
        }
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
