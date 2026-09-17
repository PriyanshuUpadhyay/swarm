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
}

pub fn parse(name: &str, text: &str) -> Result<Adapter, Box<dyn std::error::Error>> {
    let mut verbs = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
        let (key, value) = line.split_once('=').ok_or(format!("adapter {name}: bad line {line:?}"))?;
        verbs.insert(key.trim(), value.trim().to_string());
    }
    let mut take = |verb: &str| verbs.remove(verb).ok_or(format!("adapter {name}: missing {verb}"));
    let adapter = Adapter {
        name: name.to_string(),
        caller: take("self")?,
        spawn: take("spawn")?,
        ring: take("ring")?,
        list: take("list")?,
        close: take("close")?,
        capture: take("capture")?,
        attach: verbs.remove("attach"),
    };
    if let Some(key) = verbs.keys().next() {
        return Err(format!("adapter {name}: unknown key {key}").into());
    }
    Ok(adapter)
}

pub fn load(root: &std::path::Path, name: &str) -> Result<Adapter, Box<dyn std::error::Error>> {
    let path = root.join("adapters").join(format!("{name}.conf"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("adapter {name}: {}: {e}", path.display()))?;
    parse(name, &text)
}

impl Adapter {
    fn command(&self, line: &str, vars: &[(&str, &str)]) -> std::process::Command {
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(line);
        for (key, value) in vars {
            command.env(format!("SWARM_{}", key.to_uppercase()), value);
        }
        command
    }

    pub fn run(&self, verb: &str, vars: &[(&str, &str)]) -> Result<String, Box<dyn std::error::Error>> {
        let line = match verb {
            "self" => &self.caller,
            "spawn" => &self.spawn,
            "ring" => &self.ring,
            "list" => &self.list,
            "close" => &self.close,
            "capture" => &self.capture,
            _ => return Err(format!("adapter: unknown verb {verb}").into()),
        };
        let output = self.command(line, vars).output()?;
        if !output.status.success() {
            return Err(format!("{verb} failed: {}", String::from_utf8_lossy(&output.stderr).trim()).into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn attach(&self, vars: &[(&str, &str)]) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
        let line = self.attach.as_ref().ok_or_else(|| format!("swarm: adapter {} has no attach", self.name))?;
        Ok(self.command(line, vars).status()?)
    }

    /// True when `pane` appears as a whole token in the list output, so %1 never matches %12.
    pub fn has_pane(&self, pane: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let listing = self.run("list", &[])?;
        Ok(listing_has_pane(&listing, pane))
    }
}

pub fn listing_has_pane(listing: &str, pane: &str) -> bool {
    let is_id_char = |c: char| c.is_alphanumeric() || "%:_-".contains(c);
    listing.split(|c: char| !is_id_char(c)).any(|token| token == pane)
}

/// One shell line with every argument single-quoted, so spaces and quotes stay data.
pub fn shell_line(args: &[String]) -> String {
    let quoted: Vec<String> = args.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))).collect();
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
        let missing = parse("herdr", "spawn = a\nring = b\nlist = c\nclose = d\ncapture = e\n").unwrap_err().to_string();
        assert_eq!(missing, "adapter herdr: missing self");
        let unknown = parse("herdr", &format!("{FULL}dance = d\n")).unwrap_err().to_string();
        assert_eq!(unknown, "adapter herdr: unknown key dance");
        assert!(parse("herdr", "spawn\n").is_err());
    }

    #[test]
    fn parses_and_runs_optional_attach_with_inherited_status() {
        let adapter = parse("fake", &format!("{FULL}attach = exit 7\n")).unwrap();
        assert_eq!(adapter.attach.as_deref(), Some("exit 7"));
        assert_eq!(adapter.attach(&[]).unwrap().code(), Some(7));
        assert_eq!(
            parse("fake", FULL).unwrap().attach(&[]).unwrap_err().to_string(),
            "swarm: adapter fake has no attach"
        );
    }

    #[test]
    fn runs_verb_with_env_vars_and_reports_failure() {
        let fake = "self = echo current\nspawn = echo spawned $SWARM_NAME\nring = printf '%s' \"$SWARM_TEXT\"\nlist = echo a b\nclose = echo boom >&2; exit 3\ncapture = echo text of $SWARM_PANE\n";
        let adapter = parse("fake", fake).unwrap();
        assert_eq!(adapter.run("self", &[]).unwrap(), "current");
        assert_eq!(adapter.run("spawn", &[("name", "coder")]).unwrap(), "spawned coder");
        assert_eq!(adapter.run("ring", &[("text", "hi; rm -rf x")]).unwrap(), "hi; rm -rf x");
        assert_eq!(adapter.run("close", &[]).unwrap_err().to_string(), "close failed: boom");
        assert!(adapter.run("dance", &[]).is_err());
        assert_eq!(adapter.run("capture", &[("pane", "%3")]).unwrap(), "text of %3");
    }

    #[test]
    fn shell_line_quotes_every_argument() {
        let args: Vec<String> = ["echo", "a b", "it's"].map(String::from).to_vec();
        assert_eq!(shell_line(&args), "'echo' 'a b' 'it'\\''s'");
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
