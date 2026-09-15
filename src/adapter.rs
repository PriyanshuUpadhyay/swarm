#[derive(Debug)]
pub struct Adapter {
    pub spawn: String,
    pub ring: String,
    pub list: String,
    pub close: String,
}

pub fn parse(name: &str, text: &str) -> Result<Adapter, Box<dyn std::error::Error>> {
    let mut verbs = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
        let (key, value) = line.split_once('=').ok_or(format!("adapter {name}: bad line {line:?}"))?;
        verbs.insert(key.trim(), value.trim().to_string());
    }
    let mut take = |verb: &str| verbs.remove(verb).ok_or(format!("adapter {name}: missing {verb}"));
    let adapter = Adapter { spawn: take("spawn")?, ring: take("ring")?, list: take("list")?, close: take("close")? };
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
    pub fn run(&self, verb: &str, vars: &[(&str, &str)]) -> Result<String, Box<dyn std::error::Error>> {
        let line = match verb {
            "spawn" => &self.spawn,
            "ring" => &self.ring,
            "list" => &self.list,
            "close" => &self.close,
            _ => return Err(format!("adapter: unknown verb {verb}").into()),
        };
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg(line);
        for (key, value) in vars {
            command.env(format!("SWARM_{}", key.to_uppercase()), value);
        }
        let output = command.output()?;
        if !output.status.success() {
            return Err(format!("{verb} failed: {}", String::from_utf8_lossy(&output.stderr).trim()).into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// One shell line with every argument single-quoted, so spaces and quotes stay data.
pub fn shell_line(args: &[String]) -> String {
    let quoted: Vec<String> = args.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))).collect();
    quoted.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "# herdr\nspawn = herdr pane split\nring = herdr pane send-text\nlist = herdr pane list\nclose = herdr pane close\n";

    #[test]
    fn parses_four_verbs_and_rejects_missing_or_unknown() {
        let adapter = parse("herdr", FULL).unwrap();
        assert_eq!(adapter.ring, "herdr pane send-text");
        let missing = parse("herdr", "spawn = a\nring = b\nlist = c\n").unwrap_err().to_string();
        assert_eq!(missing, "adapter herdr: missing close");
        let unknown = parse("herdr", &format!("{FULL}dance = d\n")).unwrap_err().to_string();
        assert_eq!(unknown, "adapter herdr: unknown key dance");
        assert!(parse("herdr", "spawn\n").is_err());
    }

    #[test]
    fn runs_verb_with_env_vars_and_reports_failure() {
        let fake = "spawn = echo spawned $SWARM_NAME\nring = printf '%s' \"$SWARM_TEXT\"\nlist = echo a b\nclose = echo boom >&2; exit 3\n";
        let adapter = parse("fake", fake).unwrap();
        assert_eq!(adapter.run("spawn", &[("name", "coder")]).unwrap(), "spawned coder");
        assert_eq!(adapter.run("ring", &[("text", "hi; rm -rf x")]).unwrap(), "hi; rm -rf x");
        assert_eq!(adapter.run("close", &[]).unwrap_err().to_string(), "close failed: boom");
        assert!(adapter.run("dance", &[]).is_err());
    }

    #[test]
    fn shell_line_quotes_every_argument() {
        let args: Vec<String> = ["echo", "a b", "it's"].map(String::from).to_vec();
        assert_eq!(shell_line(&args), "'echo' 'a b' 'it'\\''s'");
    }
}
