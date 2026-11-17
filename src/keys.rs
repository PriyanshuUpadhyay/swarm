//! `swarm keys`: the API keys that the kit's tools read, each kept in the login Keychain as the
//! generic password `swarm-<NAME>`. This list owns which keys the Settings › Keys page shows.

use std::io::Write;
use std::process::{Command, Stdio};

pub const KEYS_SCHEMA: &str = "swarm.keys.v1";

pub struct Key {
    pub name: &'static str,
    pub label: &'static str,
    pub used_for: &'static str,
    pub get_url: &'static str,
}

/// The names `blocks/web-search/scripts/reach.py` reads with `api_key()`.
pub const KEYS: [Key; 3] = [
    Key {
        name: "TINYFISH_API_KEY",
        label: "TinyFish",
        used_for: "Web search and page reads in research (reach.py). Free daily allowance.",
        get_url: "https://agent.tinyfish.ai/api-keys",
    },
    Key {
        name: "S2_API_KEY",
        label: "Semantic Scholar",
        used_for: "Paper references and citations (reach.py paper). Optional; raises the rate limit.",
        get_url: "https://www.semanticscholar.org/product/api#api-key-form",
    },
    Key {
        name: "OPENALEX_API_KEY",
        label: "OpenAlex",
        used_for: "Paper references and citations when Semantic Scholar fails (reach.py paper). Optional.",
        get_url: "https://openalex.org/settings/api-key",
    },
];

#[derive(serde::Serialize)]
pub struct KeyStatus {
    pub name: &'static str,
    pub label: &'static str,
    pub used_for: &'static str,
    pub get_url: &'static str,
    pub saved: bool,
}

pub fn find(name: &str) -> Result<&'static Key, String> {
    KEYS.iter()
        .find(|key| key.name == name)
        .ok_or_else(|| format!("swarm: unknown key {name}; known keys: {}", names()))
}

fn names() -> String {
    KEYS.iter()
        .map(|key| key.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `SWARM_SECURITY_CMD` is the stand-in the web e2e server sets, so no case reaches the real Keychain.
fn security() -> String {
    std::env::var("SWARM_SECURITY_CMD").unwrap_or_else(|_| "security".into())
}

fn service(key: &Key) -> String {
    format!("swarm-{}", key.name)
}

/// Whether the Keychain holds the key. This reads the item's attributes only, never its value,
/// so it needs no Keychain access prompt.
fn saved(key: &Key) -> bool {
    Command::new(security())
        .args(["find-generic-password", "-s", &service(key)])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn list() -> Vec<KeyStatus> {
    KEYS.iter()
        .map(|key| KeyStatus {
            name: key.name,
            label: key.label,
            used_for: key.used_for,
            get_url: key.get_url,
            saved: saved(key),
        })
        .collect()
}

/// The value goes inside double quotes on `security -i`'s stdin, so a quote, a backslash, or any
/// whitespace or control character could end it early and add a command. API keys use none of them.
pub fn check_value(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 1024 {
        return Err("swarm: a key is 1 to 1024 characters".into());
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
    {
        return Err(
            "swarm: a key may not hold spaces, quotes, backslashes, or non-ASCII characters".into(),
        );
    }
    Ok(())
}

/// Adds or replaces the key. The value goes on stdin, not argv, so no process list shows it.
pub fn save(name: &str, value: &str) -> Result<(), String> {
    let key = find(name)?;
    check_value(value)?;
    let account = std::env::var("USER").unwrap_or_else(|_| "swarm".into());
    if account
        .bytes()
        .any(|b| !b.is_ascii_alphanumeric() && !b"._-".contains(&b))
    {
        return Err("swarm: the USER name is not a plain account name".into());
    }
    let mut child = Command::new(security())
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("swarm: could not run security: {err}"))?;
    let line = format!(
        "add-generic-password -U -a \"{account}\" -s \"{}\" -w \"{value}\"\n",
        service(key)
    );
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin
        .write_all(line.as_bytes())
        .map_err(|err| format!("swarm: could not write to security: {err}"))?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|err| format!("swarm: security failed: {err}"))?;
    // `security -i` exits 0 when a command in it fails, so the item itself is the proof.
    if !saved(key) {
        let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!("swarm: the Keychain did not save {name}: {reason}"));
    }
    Ok(())
}

/// Removes the key. A key that is not saved is already removed.
pub fn remove(name: &str) -> Result<(), String> {
    let key = find(name)?;
    let output = Command::new(security())
        .args(["delete-generic-password", "-s", &service(key)])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("swarm: could not run security: {err}"))?;
    if saved(key) {
        let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "swarm: the Keychain did not remove {name}: {reason}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_that_could_end_the_quoted_argument_is_refused() {
        assert!(check_value("tf_live-AbC.123=+/:").is_ok());
        for value in ["a\" -s other", "a\\b", "a b", "a\nb", ""] {
            assert!(check_value(value).is_err(), "{value:?}");
        }
    }
}
