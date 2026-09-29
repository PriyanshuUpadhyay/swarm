const SWARM_DIR: &str = ".swarm";
const SWARM_DB: &str = "swarm.db";

/// The parent of the `.swarm` data directory for this build. See ADR 0027.
///
/// SWARM_HOME is read first, so an explicit one works when HOME is unset or not UTF-8.
pub fn home() -> Result<String, Box<dyn std::error::Error>> {
    match std::env::var("SWARM_HOME") {
        Err(std::env::VarError::NotPresent) => Ok(branch_home(
            &std::env::var("HOME")?,
            env!("SWARM_BUILD_BRANCH"),
        )),
        explicit => Ok(explicit?),
    }
}

/// HOME for a `main` build or a build with no branch (`""`), else `HOME/<branch_folder>`.
pub fn branch_home(home: &str, branch: &str) -> String {
    match branch_folder(branch) {
        None => home.to_string(),
        Some(folder) => std::path::Path::new(home)
            .join(folder)
            .to_string_lossy()
            .into_owned(),
    }
}

/// The folder a branch build keeps its data in; `ui/Sources/SwarmCore/System/SwarmHome.swift`
/// must give the same bytes. A branch made only of `[A-Za-z0-9._-]` that does not start with `.`
/// or `-` is `.swarm-<branch>`. Any other branch is `.swarm-<slug>+<hash>`: the slug maps each
/// UTF-8 byte outside that set to `-`, and the hash is the 32-bit FNV-1a of the branch's UTF-8
/// bytes as 8 lowercase hex digits. A safe name never holds `+`, so the two forms cannot meet, and
/// the hash keeps `feat/login` apart from `feat-login`.
pub fn branch_folder(branch: &str) -> Option<String> {
    if matches!(branch, "" | "main") {
        return None;
    }
    let safe = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-');
    let bytes = branch.as_bytes();
    if bytes.iter().all(|&byte| safe(byte)) && !matches!(bytes[0], b'.' | b'-') {
        return Some(format!(".swarm-{branch}"));
    }
    let slug: String = bytes
        .iter()
        .map(|&byte| if safe(byte) { byte as char } else { '-' })
        .collect();
    let hash = bytes.iter().fold(0x811c_9dc5_u32, |hash, &byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    Some(format!(".swarm-{slug}+{hash:08x}"))
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
    use super::{branch_folder, branch_home};

    /// Shared with `ui/Tests/SwarmCoreTests/SwarmHomeTests.swift`; keep both lists the same.
    const VECTORS: [(&str, Option<&str>); 11] = [
        ("main", None),
        ("", None),
        ("unknown", Some(".swarm-unknown")),
        ("ui-polish", Some(".swarm-ui-polish")),
        ("feat/login", Some(".swarm-feat-login+a15997df")),
        ("feat-login", Some(".swarm-feat-login")),
        ("feat/👩‍💻", Some(".swarm-feat------------+2df12934")),
        ("..", Some(".swarm-..+a3d4a70d")),
        ("-x", Some(".swarm--x+4bcd60c0")),
        ("\"main\"", Some(".swarm--main-+0c126bfe")),
        ("a'b", Some(".swarm-a-b+2aa1e449")),
    ];

    #[test]
    fn branch_folders_match_the_shared_vectors() {
        for (branch, folder) in VECTORS {
            assert_eq!(
                branch_folder(branch).as_deref(),
                folder,
                "branch {branch:?}"
            );
        }
    }

    #[test]
    fn main_and_no_branch_use_home_and_others_a_folder_in_it() {
        assert_eq!(branch_home("/home-dir", "main"), "/home-dir");
        assert_eq!(branch_home("/home-dir", ""), "/home-dir");
        assert_eq!(
            branch_home("/home-dir", "ui-polish"),
            "/home-dir/.swarm-ui-polish"
        );
    }
}
