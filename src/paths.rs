const SWARM_DIR: &str = ".swarm";
const SWARM_DB: &str = "swarm.db";

/// The parent of the `.swarm` data directory for this build. See ADR 0027.
pub fn home() -> Result<String, Box<dyn std::error::Error>> {
    Ok(resolve_home(
        std::env::var("SWARM_HOME").ok().as_deref(),
        &std::env::var("HOME")?,
        env!("SWARM_BUILD_BRANCH"),
    ))
}

/// An explicit SWARM_HOME wins. Else a `main` build, a detached build, or a build with no branch
/// uses HOME, and any other branch uses `HOME/.swarm-<branch>`, with every character outside
/// `[A-Za-z0-9._-]` made a `-` so the branch is one safe folder name.
pub fn resolve_home(swarm_home: Option<&str>, home: &str, branch: &str) -> String {
    if let Some(explicit) = swarm_home {
        return explicit.to_string();
    }
    if matches!(branch, "" | "main" | "HEAD" | "unknown") {
        return home.to_string();
    }
    let safe: String = branch
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '_' | '-' => c,
            _ => '-',
        })
        .collect();
    std::path::Path::new(home)
        .join(format!(".swarm-{safe}"))
        .to_string_lossy()
        .into_owned()
}

pub fn root_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(std::path::PathBuf::from(home()?).join(SWARM_DIR))
}

pub fn runs_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(root_dir()?.join("runs"))
}

pub fn sqlite_db() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(root_dir()?.join(SWARM_DB))
}

#[cfg(test)]
mod tests {
    use super::resolve_home;

    #[test]
    fn main_detached_and_unknown_builds_use_home() {
        for branch in ["main", "HEAD", "", "unknown"] {
            assert_eq!(resolve_home(None, "/home-dir", branch), "/home-dir");
        }
    }

    #[test]
    fn feature_branch_uses_its_own_home() {
        assert_eq!(
            resolve_home(None, "/home-dir", "ui-polish"),
            "/home-dir/.swarm-ui-polish"
        );
    }

    #[test]
    fn unsafe_branch_characters_become_dashes() {
        assert_eq!(
            resolve_home(None, "/home-dir", "feat/x y"),
            "/home-dir/.swarm-feat-x-y"
        );
    }

    #[test]
    fn explicit_swarm_home_wins() {
        assert_eq!(
            resolve_home(Some("/tmp/explicit"), "/home-dir", "ui-polish"),
            "/tmp/explicit"
        );
        assert_eq!(
            resolve_home(Some("/tmp/explicit"), "/home-dir", "main"),
            "/tmp/explicit"
        );
    }
}
