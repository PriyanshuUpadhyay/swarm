// Adapter command execution, bounded reads, and shell argument handling.

use super::Adapter;

impl Adapter {
    fn command(&self, line: &str, vars: &[(&str, &str)]) -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(line);
        if let Some(session_id) = &self.session_id {
            command
                .env("SWARM_SESSION_ID", session_id)
                .env("SWARM_AGENT_ID", "orchestrator")
                .env("SWARM_ADAPTER", &self.name);
        }
        // A verb that calls swarm back runs this same binary, even from an app with a short PATH.
        if let Ok(exe) = std::env::current_exe() {
            command.env("SWARM_EXE", exe);
        }
        for (key, value) in vars {
            command.env(format!("SWARM_{}", key.to_uppercase()), value);
        }
        // tmux runs a `pipe-pane` line with its server's environment, which another session or
        // build may have started, so the feed line names its own binary, ids, and home.
        let var = |name: &str| vars.iter().find(|(key, _)| *key == name).map(|(_, v)| *v);
        if let (Ok(exe), Some(session), Some(agent), Some(home)) = (
            std::env::current_exe(),
            var("session_id"),
            var("agent_id"),
            var("home"),
        ) {
            command.env(
                "SWARM_FEED",
                shell_line(&[
                    "exec".into(),
                    "env".into(),
                    format!("SWARM_SESSION_ID={session}"),
                    format!("SWARM_AGENT_ID={agent}"),
                    format!("SWARM_HOME={home}"),
                    exe.to_string_lossy().into_owned(),
                    "status-feed".into(),
                ]),
            );
        }
        command
    }

    pub fn run(
        &self,
        verb: &str,
        vars: &[(&str, &str)],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let line = match verb {
            "self" => &self.caller,
            "spawn" => &self.spawn,
            "ring" => &self.ring,
            "list" => &self.list,
            "close" => &self.close,
            "capture" => &self.capture,
            "focus" => self
                .focus
                .as_ref()
                .ok_or_else(|| format!("swarm: {} has no focus", self.name))?,
            "interrupt" => self
                .interrupt
                .as_ref()
                .ok_or_else(|| format!("swarm: adapter {} has no interrupt", self.name))?,
            "key" => self.key.as_ref().ok_or_else(|| {
                format!(
                    "swarm: adapter {} cannot send a key, so answer in the pane",
                    self.name
                )
            })?,
            "client" => self
                .client
                .as_ref()
                .ok_or_else(|| format!("swarm: {} has no client", self.name))?,
            "run" => self
                .run_line
                .as_ref()
                .ok_or_else(|| format!("swarm: {} has no run", self.name))?,
            _ => return Err(format!("adapter: unknown verb {verb}").into()),
        };
        if let Some(deadline) = self.deadline {
            self.check_deadline()?;
            return self
                .read_bounded(
                    line,
                    vars,
                    deadline.saturating_duration_since(std::time::Instant::now()),
                )
                .map(|text| text.trim().to_string())
                .map_err(|error| format!("{verb} failed: {error}").into());
        }
        let output = self.command(line, vars).output()?;
        if !output.status.success() {
            return Err(format!(
                "{verb} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// The `screen` verb's output, or None when the adapter has none, it fails, or it takes
    /// longer than `timeout`. The listing calls it once per live agent, so a slow pane must not
    /// hold the listing up.
    pub fn screen(&self, vars: &[(&str, &str)], timeout: std::time::Duration) -> Option<String> {
        self.read_bounded(self.screen.as_ref()?, vars, timeout).ok()
    }

    /// The `status` verb's record, or None when the adapter has none, it fails, it takes longer
    /// than `timeout`, or the agent reported no record.
    pub fn status(&self, vars: &[(&str, &str)], timeout: std::time::Duration) -> Option<String> {
        let record = self
            .read_bounded(self.status.as_ref()?, vars, timeout)
            .ok()?;
        Some(record.trim().to_string()).filter(|record| !record.is_empty())
    }

    /// The `capture` verb's output under the same limit as `screen`.
    pub fn capture_within(
        &self,
        vars: &[(&str, &str)],
        timeout: std::time::Duration,
    ) -> Option<String> {
        self.read_bounded(&self.capture, vars, timeout).ok()
    }

    pub fn check_deadline(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "adapter session timed out",
            )
            .into());
        }
        Ok(())
    }

    fn read_bounded(
        &self,
        line: &str,
        vars: &[(&str, &str)],
        timeout: std::time::Duration,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use std::os::unix::process::CommandExt;
        self.check_deadline()?;
        let started = std::time::Instant::now();
        let deadline = self
            .deadline
            .map_or(started + timeout, |session| session.min(started + timeout));
        // Its own process group, so the deadline can also stop a grandchild that holds stdout.
        let mut child = self
            .command(line, vars)
            .stdout(std::process::Stdio::piped())
            .stderr(if self.deadline.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .process_group(0)
            .spawn()?;
        let group = child.id() as i32;
        let errors = child.stderr.take().map(|mut stderr| {
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
                let _ = sender.send(text);
            });
            receiver
        });
        let mut stdout = child.stdout.take().expect("piped stdout");
        // The read runs apart from the deadline: a child that exits while a grandchild keeps the
        // pipe open would otherwise block read_to_end past it.
        let (sender, output) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = std::io::Read::read_to_end(&mut stdout, &mut bytes)
                .map(|_| String::from_utf8_lossy(&bytes).into_owned());
            let _ = sender.send(read);
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                _ => break None,
            }
        };
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let text = match status {
            Some(status) if status.success() => match output.recv_timeout(remaining) {
                Ok(read) => read.ok(),
                Err(_) => {
                    // Output is complete once the verb exits; stop what holds the pipe.
                    kill_group(group);
                    output
                        .recv_timeout(std::time::Duration::from_millis(100))
                        .ok()
                        .and_then(Result::ok)
                }
            },
            _ => None,
        };
        kill_group(group);
        let _ = child.kill();
        let _ = child.wait();
        self.check_deadline()?;
        match status {
            Some(status) if !status.success() => {
                let detail = errors
                    .and_then(|errors| {
                        errors
                            .recv_timeout(
                                deadline.saturating_duration_since(std::time::Instant::now()),
                            )
                            .ok()
                    })
                    .unwrap_or_default();
                Err(format!("adapter command failed: {}", detail.trim()).into())
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "adapter read timed out",
            )
            .into()),
            _ => text.ok_or_else(|| "adapter output unavailable".into()),
        }
    }

    pub fn attach(
        &self,
        vars: &[(&str, &str)],
    ) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
        Ok(self.attach_command(vars)?.status()?)
    }

    /// The `attach` line as a command, for a caller that runs it in its own terminal.
    pub fn attach_command(
        &self,
        vars: &[(&str, &str)],
    ) -> Result<std::process::Command, Box<dyn std::error::Error>> {
        let line = self
            .attach
            .as_ref()
            .ok_or_else(|| format!("swarm: adapter {} has no attach", self.name))?;
        Ok(self.command(line, vars))
    }

    /// True when `pane` appears as a whole token in the list output, so %1 never matches %12.
    pub fn has_pane(&self, pane: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let listing = self.run("list", &[])?;
        Ok(listing_has_pane(&listing, pane))
    }
}

/// Sends SIGKILL to a whole process group. A group that has already exited is fine.
fn kill_group(group: i32) {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    const SIGKILL: i32 = 9;
    // SAFETY: kill(2) takes plain integers and has no memory effects on this process; a negative
    // pid names the group that `process_group(0)` gave the child.
    unsafe {
        kill(-group, SIGKILL);
    }
}

pub fn listing_has_pane(listing: &str, pane: &str) -> bool {
    let is_id_char = |c: char| c.is_alphanumeric() || "%:_-".contains(c);
    listing
        .split(|c: char| !is_id_char(c))
        .any(|token| token == pane)
}

/// One shell line with every argument single-quoted, so spaces and quotes stay data.
pub fn shell_line(args: &[String]) -> String {
    let quoted: Vec<String> = args
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
        .collect();
    quoted.join(" ")
}
