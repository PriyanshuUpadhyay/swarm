#[derive(Debug)]
pub struct Adapter {
    pub name: String,
    pub caller: String,
    pub spawn: String,
    pub ring: String,
    pub list: String,
    pub close: String,
    pub capture: String,
    pub attach: Option<String>,
    pub interrupt: Option<String>,
    /// Prints a pane's visible rows, or Herdr's JSON for the agent in it (ADR 0021).
    pub screen: Option<String>,
    /// Sends one key name, such as `2`, `Down`, or `Enter`, to a pane (`swarm answer`).
    pub key: Option<String>,
}

/// The adapters this binary carries, which are the ones it is tested against.
///
/// **A shipped adapter lives here and not on disk.** `swarm init` used to write these files and
/// then leave them alone for ever, so `adapters/tmux-solo.conf` changing `self` from `true` to the
/// pane id reached the repository and never reached the machine: `agent add` recorded no pane and
/// every `type` answered `no pane recorded`. A deployed file is an edit now, and it overrides only
/// the verbs it names, so one edited verb cannot hold back a fix to any other. See `load`.
pub const SHIPPED: [(&str, &str); 3] = [
    ("tmux", include_str!("../adapters/tmux.conf")),
    ("tmux-solo", include_str!("../adapters/tmux-solo.conf")),
    ("herdr", include_str!("../adapters/herdr.conf")),
];

pub fn shipped(name: &str) -> Option<&'static str> {
    SHIPPED
        .iter()
        .find(|(shipped, _)| *shipped == name)
        .map(|(_, text)| *text)
}

type Verbs = std::collections::HashMap<String, String>;

fn verbs(name: &str, text: &str) -> Result<Verbs, Box<dyn std::error::Error>> {
    let mut verbs = Verbs::new();
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
    {
        let (key, value) = line
            .split_once('=')
            .ok_or(format!("adapter {name}: bad line {line:?}"))?;
        verbs.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(verbs)
}

/// The verbs a deployed file states differently from the text shipped for the same adapter.
pub fn overrides(name: &str, deployed: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let Some(base) = shipped(name) else {
        return Ok(Vec::new());
    };
    let base = verbs(name, base)?;
    let mut named: Vec<String> = verbs(name, deployed)?
        .into_iter()
        .filter(|(key, value)| base.get(key) != Some(value))
        .map(|(key, _)| key)
        .collect();
    named.sort();
    Ok(named)
}

pub fn parse(name: &str, text: &str) -> Result<Adapter, Box<dyn std::error::Error>> {
    build(name, verbs(name, text)?)
}

fn build(name: &str, mut verbs: Verbs) -> Result<Adapter, Box<dyn std::error::Error>> {
    let mut take = |verb: &str| {
        verbs
            .remove(verb)
            .ok_or(format!("adapter {name}: missing {verb}"))
    };
    let adapter = Adapter {
        name: name.to_string(),
        caller: take("self")?,
        spawn: take("spawn")?,
        ring: take("ring")?,
        list: take("list")?,
        close: take("close")?,
        capture: take("capture")?,
        attach: verbs.remove("attach"),
        interrupt: verbs.remove("interrupt"),
        screen: verbs.remove("screen"),
        key: verbs.remove("key"),
    };
    if let Some(key) = verbs.keys().next() {
        return Err(format!("adapter {name}: unknown key {key}").into());
    }
    Ok(adapter)
}

/// The shipped text, with a deployed file laid over the verbs it names.
///
/// An adapter this binary does not ship has to exist on disk, which is how a machine adds one.
pub fn load(root: &std::path::Path, name: &str) -> Result<Adapter, Box<dyn std::error::Error>> {
    let path = root.join("adapters").join(format!("{name}.conf"));
    let deployed = std::fs::read_to_string(&path);
    match (shipped(name), deployed) {
        (Some(base), Ok(text)) => {
            let mut merged = verbs(name, base)?;
            merged.extend(verbs(name, &text)?);
            build(name, merged)
        }
        (Some(base), Err(_)) => parse(name, base),
        (None, Ok(text)) => parse(name, &text),
        (None, Err(error)) => Err(format!("adapter {name}: {}: {error}", path.display()).into()),
    }
}

impl Adapter {
    fn command(&self, line: &str, vars: &[(&str, &str)]) -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(line);
        // A verb that calls swarm back runs this same binary, even from an app with a short PATH.
        if let Ok(exe) = std::env::current_exe() {
            command.env("SWARM_EXE", exe);
        }
        for (key, value) in vars {
            command.env(format!("SWARM_{}", key.to_uppercase()), value);
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
            _ => return Err(format!("adapter: unknown verb {verb}").into()),
        };
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
        self.read_bounded(self.screen.as_ref()?, vars, timeout)
    }

    /// The `capture` verb's output under the same limit as `screen`.
    pub fn capture_within(
        &self,
        vars: &[(&str, &str)],
        timeout: std::time::Duration,
    ) -> Option<String> {
        self.read_bounded(&self.capture, vars, timeout)
    }

    fn read_bounded(
        &self,
        line: &str,
        vars: &[(&str, &str)],
        timeout: std::time::Duration,
    ) -> Option<String> {
        use std::os::unix::process::CommandExt;
        // Its own process group, so the deadline can also stop a grandchild that holds stdout.
        let mut child = self
            .command(line, vars)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .ok()?;
        let group = child.id() as i32;
        let mut stdout = child.stdout.take()?;
        // The read runs apart from the deadline: a child that exits while a grandchild keeps the
        // pipe open would otherwise block read_to_string past it.
        let (sender, output) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let read = std::io::Read::read_to_string(&mut stdout, &mut text).map(|_| text);
            let _ = sender.send(read);
        });
        let deadline = std::time::Instant::now() + timeout;
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
        text
    }

    pub fn attach(
        &self,
        vars: &[(&str, &str)],
    ) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
        let line = self
            .attach
            .as_ref()
            .ok_or_else(|| format!("swarm: adapter {} has no attach", self.name))?;
        Ok(self.command(line, vars).status()?)
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

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "# herdr\nself = printf '%s' \"$HERDR_PANE_ID\"\nspawn = herdr pane split\nring = herdr pane send-text\nlist = herdr pane list\nclose = herdr pane close\ncapture = herdr pane read\n";

    #[test]
    fn parses_verbs_and_rejects_missing_or_unknown() {
        let adapter = parse("herdr", FULL).unwrap();
        assert_eq!(adapter.ring, "herdr pane send-text");
        assert_eq!(adapter.name, "herdr");
        assert_eq!(adapter.attach, None);
        assert_eq!(adapter.interrupt, None);
        let missing = parse(
            "herdr",
            "spawn = a\nring = b\nlist = c\nclose = d\ncapture = e\n",
        )
        .unwrap_err()
        .to_string();
        assert_eq!(missing, "adapter herdr: missing self");
        let unknown = parse("herdr", &format!("{FULL}dance = d\n"))
            .unwrap_err()
            .to_string();
        assert_eq!(unknown, "adapter herdr: unknown key dance");
        assert!(parse("herdr", "spawn\n").is_err());
    }

    #[test]
    fn parses_and_runs_optional_attach_with_inherited_status() {
        let adapter = parse("fake", &format!("{FULL}attach = exit 7\n")).unwrap();
        assert_eq!(adapter.attach.as_deref(), Some("exit 7"));
        assert_eq!(adapter.attach(&[]).unwrap().code(), Some(7));
        assert_eq!(
            parse("fake", FULL)
                .unwrap()
                .attach(&[])
                .unwrap_err()
                .to_string(),
            "swarm: adapter fake has no attach"
        );
    }

    #[test]
    fn shipped_herdr_attaches_by_agent_pane() {
        let adapter = parse("herdr", shipped("herdr").unwrap()).unwrap();
        assert_eq!(
            adapter.attach.as_deref(),
            Some("herdr agent attach \"$SWARM_PANE\"")
        );
    }

    #[test]
    fn parses_and_runs_optional_interrupt() {
        let adapter = parse("fake", &format!("{FULL}interrupt = printf interrupted\n")).unwrap();
        assert_eq!(adapter.run("interrupt", &[]).unwrap(), "interrupted");
        assert_eq!(
            parse("fake", FULL)
                .unwrap()
                .run("interrupt", &[])
                .unwrap_err()
                .to_string(),
            "swarm: adapter fake has no interrupt"
        );
    }

    #[test]
    fn runs_verb_with_env_vars_and_reports_failure() {
        let fake = "self = echo current\nspawn = echo spawned $SWARM_NAME\nring = printf '%s' \"$SWARM_TEXT\"\nlist = echo a b\nclose = echo boom >&2; exit 3\ncapture = echo text of $SWARM_PANE\n";
        let adapter = parse("fake", fake).unwrap();
        assert_eq!(adapter.run("self", &[]).unwrap(), "current");
        assert_eq!(
            adapter.run("spawn", &[("name", "coder")]).unwrap(),
            "spawned coder"
        );
        assert_eq!(
            adapter.run("ring", &[("text", "hi; rm -rf x")]).unwrap(),
            "hi; rm -rf x"
        );
        assert_eq!(
            adapter.run("close", &[]).unwrap_err().to_string(),
            "close failed: boom"
        );
        assert!(adapter.run("dance", &[]).is_err());
        assert_eq!(
            adapter.run("capture", &[("pane", "%3")]).unwrap(),
            "text of %3"
        );
    }

    #[test]
    fn screen_gives_up_on_a_slow_or_failing_verb() {
        let with = |line: &str| parse("probe", &format!("{FULL}screen = {line}\n")).unwrap();
        let timeout = std::time::Duration::from_millis(300);
        assert_eq!(
            with(r#"printf '%s' "rows of $SWARM_PANE""#).screen(&[("pane", "%3")], timeout),
            Some("rows of %3".into())
        );
        let started = std::time::Instant::now();
        assert_eq!(with("exec sleep 5").screen(&[], timeout), None);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(with("exit 1").screen(&[], timeout), None);
        assert_eq!(parse("probe", FULL).unwrap().screen(&[], timeout), None);
    }

    #[test]
    fn screen_returns_by_the_deadline_when_a_child_keeps_stdout_open() {
        let adapter = parse("probe", &format!("{FULL}screen = sleep 5 & printf rows\n")).unwrap();
        let started = std::time::Instant::now();
        let text = adapter.screen(&[], std::time::Duration::from_millis(300));
        assert!(started.elapsed() < std::time::Duration::from_millis(700));
        assert_eq!(text.as_deref(), Some("rows"));
    }

    #[test]
    fn shell_line_quotes_every_argument() {
        let args: Vec<String> = ["echo", "a b", "it's"].map(String::from).to_vec();
        assert_eq!(shell_line(&args), "'echo' 'a b' 'it'\\''s'");
    }

    /// The fault of 2026-09-21. `adapters/tmux-solo.conf` changed `self` to print the pane id, the
    /// deployed copy kept `self = true`, and every chat answered `no pane recorded` at its first
    /// message. A deployed file now speaks for the verbs it names and for nothing else.
    #[test]
    fn a_deployed_file_overrides_only_the_verbs_it_names() {
        let root = std::env::temp_dir().join(format!("swarm-adapter-{}", std::process::id()));
        let adapters = root.join("adapters");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();

        let base = parse("herdr", shipped("herdr").unwrap()).unwrap();
        assert_eq!(load(&root, "herdr").unwrap().caller, base.caller);

        std::fs::write(adapters.join("herdr.conf"), "spawn = python3 split.py\n").unwrap();
        let merged = load(&root, "herdr").unwrap();
        assert_eq!(merged.spawn, "python3 split.py");
        assert_eq!(merged.caller, base.caller);
        assert_eq!(
            overrides("herdr", "spawn = python3 split.py\n").unwrap(),
            ["spawn"]
        );
        assert!(
            overrides("herdr", shipped("herdr").unwrap())
                .unwrap()
                .is_empty()
        );

        std::fs::write(adapters.join("own.conf"), FULL).unwrap();
        assert_eq!(load(&root, "own").unwrap().ring, "herdr pane send-text");
        assert!(
            load(&root, "missing")
                .unwrap_err()
                .to_string()
                .contains("missing.conf")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn has_pane_matches_whole_tokens_only() {
        let fake = "self = s\nspawn = a\nring = b\nlist = echo '%1 zsh %12 zsh \"w8A:p2\"'\nclose = d\ncapture = e\n";
        let adapter = parse("fake", fake).unwrap();
        assert!(adapter.has_pane("%1").unwrap());
        assert!(adapter.has_pane("%12").unwrap());
        assert!(adapter.has_pane("w8A:p2").unwrap());
        assert!(!adapter.has_pane("%2").unwrap());
    }
}
