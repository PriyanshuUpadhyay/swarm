mod runtime;

pub use runtime::{listing_has_pane, shell_line};

#[derive(Clone, Debug)]
pub struct Adapter {
    pub name: String,
    /// A multi-session read sets this so adapter scripts see that session's UI caller.
    pub session_id: Option<String>,
    /// A batch session shares this deadline across all adapter reads.
    pub deadline: Option<std::time::Instant>,
    pub caller: String,
    pub spawn: String,
    pub ring: String,
    pub list: String,
    pub close: String,
    pub capture: String,
    pub attach: Option<String>,
    /// Brings the owner to `$SWARM_PANE` in the host (`swarm focus`).
    pub focus: Option<String>,
    pub interrupt: Option<String>,
    /// Prints a pane's visible rows, or Herdr's JSON for the agent in it (ADR 0021).
    pub screen: Option<String>,
    /// Sends one key name, such as `2`, `Down`, or `Enter`, to a pane (`swarm answer`).
    pub key: Option<String>,
    /// Prints the pid of the host client, for window activation after focus.
    pub client: Option<String>,
    /// Opens a pane whose program is `$SWARM_COMMAND`, the launch line `spawn` would ring, and
    /// prints its id. With it, `launch` does not ring the line into a shell.
    pub run_line: Option<String>,
    /// Prints the root program-status record of the agent in `$SWARM_PANE` as one OSC 7501 body
    /// with `:at=<unix seconds>`, the record's own time, or nothing when it reported none.
    pub status: Option<String>,
}

/// The adapters this binary carries, which are the ones it is tested against.
///
/// **A shipped adapter lives here and not on disk.** `swarm init` used to write these files and
/// then leave them alone for ever, so `adapters/tmux-swarm.conf` changing `self` from `true` to the
/// pane id reached the repository and never reached the machine: `agent add` recorded no pane and
/// every `type` answered `no pane recorded`. A deployed file is an edit now, and it overrides only
/// the verbs it names, so one edited verb cannot hold back a fix to any other. See `load`.
pub const SHIPPED: [(&str, &str); 5] = [
    ("tmux", include_str!("../adapters/tmux.conf")),
    ("tmux-swarm", include_str!("../adapters/tmux-swarm.conf")),
    ("herdr", include_str!("../adapters/herdr.conf")),
    ("superset", include_str!("../adapters/superset.conf")),
    ("rex", include_str!("../adapters/rex.conf")),
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
        session_id: None,
        deadline: None,
        caller: take("self")?,
        spawn: take("spawn")?,
        ring: take("ring")?,
        list: take("list")?,
        close: take("close")?,
        capture: take("capture")?,
        attach: verbs.remove("attach"),
        focus: verbs.remove("focus"),
        interrupt: verbs.remove("interrupt"),
        screen: verbs.remove("screen"),
        key: verbs.remove("key"),
        client: verbs.remove("client"),
        run_line: verbs.remove("run"),
        status: verbs.remove("status"),
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
            let deployed = verbs(name, &text)?;
            // `run` stands in for the shipped `spawn` and needs the shipped `list`, which leaves an
            // ended pane out. A machine's own `spawn` or `list` stays in force unless its file
            // names `run` too.
            let changed = |verb: &str| {
                deployed
                    .get(verb)
                    .is_some_and(|line| merged.get(verb) != Some(line))
            };
            if (changed("spawn") || changed("list")) && !deployed.contains_key("run") {
                merged.remove("run");
            }
            merged.extend(deployed);
            build(name, merged)
        }
        (Some(base), Err(_)) => parse(name, base),
        (None, Ok(text)) => parse(name, &text),
        (None, Err(error)) => Err(format!("adapter {name}: {}: {error}", path.display()).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Long enough that only a hung verb meets it, however slowly a loaded machine starts one.
    const LONG: std::time::Duration = std::time::Duration::from_secs(30);

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
    fn shipped_superset_has_the_required_verbs_and_screen_only() {
        let adapter = parse("superset", shipped("superset").unwrap()).unwrap();
        assert!(adapter.attach.is_none());
        assert!(adapter.interrupt.is_none());
        assert!(adapter.key.is_none());
        assert!(adapter.screen.is_some());
        assert!(adapter.caller.contains("SUPERSET_TERMINAL_ID"));
    }

    #[test]
    fn shipped_rex_has_the_required_verbs_and_key_but_no_attach() {
        let adapter = parse("rex", shipped("rex").unwrap()).unwrap();
        assert!(adapter.attach.is_none());
        assert!(adapter.interrupt.is_some());
        assert!(adapter.key.is_some());
        assert!(adapter.screen.is_some());
        assert!(adapter.status.is_some());
        assert!(adapter.caller.contains("REX_BLOCK"));
    }

    #[test]
    fn the_status_verb_is_optional_and_no_record_is_none() {
        let status = |line: &str| {
            parse("fake", &format!("{FULL}status = {line}\n"))
                .unwrap()
                .status(&[("pane", "%3")], LONG)
        };
        assert_eq!(
            status("printf 'state=working:at=7 %s' \"$SWARM_PANE\"").as_deref(),
            Some("state=working:at=7 %3")
        );
        assert_eq!(status("true"), None);
        assert_eq!(status("printf 'state=done:at=7'; exit 1"), None);
        assert_eq!(parse("fake", FULL).unwrap().status(&[], LONG), None);
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
    fn bounded_list_decodes_non_utf8_like_unbounded_list() {
        let mut adapter = parse("probe", FULL).unwrap();
        adapter.list = "printf 'pane \\351'".into();
        let expected = adapter.run("list", &[]).unwrap();
        adapter.deadline = Some(std::time::Instant::now() + LONG);
        assert_eq!(adapter.run("list", &[]).unwrap(), expected);
        assert_eq!(expected, "pane \u{fffd}");
    }

    #[test]
    fn screen_gives_up_on_a_slow_or_failing_verb() {
        let with = |line: &str| parse("probe", &format!("{FULL}screen = {line}\n")).unwrap();
        assert_eq!(
            with(r#"printf '%s' "rows of $SWARM_PANE""#).screen(&[("pane", "%3")], LONG),
            Some("rows of %3".into())
        );
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(300);
        assert_eq!(with("exec sleep 30").screen(&[], timeout), None);
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        assert_eq!(with("exit 1").screen(&[], LONG), None);
        assert_eq!(parse("probe", FULL).unwrap().screen(&[], LONG), None);
    }

    #[test]
    fn screen_returns_by_the_deadline_when_a_child_keeps_stdout_open() {
        let adapter = parse("probe", &format!("{FULL}screen = sleep 30 & printf rows\n")).unwrap();
        let started = std::time::Instant::now();
        // The shell must print and exit within the timeout, which a loaded machine can delay.
        let text = adapter.screen(&[], std::time::Duration::from_secs(5));
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        assert_eq!(text.as_deref(), Some("rows"));
    }

    #[test]
    fn shell_line_quotes_every_argument() {
        let args: Vec<String> = ["echo", "a b", "it's"].map(String::from).to_vec();
        assert_eq!(shell_line(&args), "'echo' 'a b' 'it'\\''s'");
    }

    /// The fault of 2026-09-21. `adapters/tmux-swarm.conf` changed `self` to print the pane id, the
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

    /// `run` stands in for `spawn` and needs the shipped `list`, so a machine's own `spawn` or
    /// `list` stays in force: the shipped `run` drops out unless the file names `run` too.
    #[test]
    fn a_deployed_spawn_turns_off_the_shipped_run() {
        let root = std::env::temp_dir().join(format!("swarm-adapter-run-{}", std::process::id()));
        let adapters = root.join("adapters");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&adapters).unwrap();
        assert!(load(&root, "rex").unwrap().run_line.is_some());
        std::fs::write(adapters.join("rex.conf"), "spawn = own-split\n").unwrap();
        let edited = load(&root, "rex").unwrap();
        assert_eq!(edited.spawn, "own-split");
        assert_eq!(edited.run_line, None);
        std::fs::write(adapters.join("rex.conf"), "list = own-list\n").unwrap();
        assert_eq!(load(&root, "rex").unwrap().run_line, None);
        // A full copy of the shipped file changes nothing, so it keeps the shipped `run`.
        std::fs::write(adapters.join("rex.conf"), shipped("rex").unwrap()).unwrap();
        assert!(load(&root, "rex").unwrap().run_line.is_some());
        std::fs::write(
            adapters.join("rex.conf"),
            "spawn = own-split\nrun = own-run\n",
        )
        .unwrap();
        assert_eq!(
            load(&root, "rex").unwrap().run_line.as_deref(),
            Some("own-run")
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
