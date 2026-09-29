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
/// must give the same bytes. A branch of at most 200 bytes made only of `[A-Za-z0-9._-]` that does
/// not start with `.` or `-` is `.swarm-<branch>`, whole. Any other branch is
/// `.swarm-<slug>+<hash>`: the slug maps each UTF-8 byte outside that set to `-` and keeps at most
/// 200 bytes, and the hash is the 64-bit FNV-1a of the whole branch's UTF-8 bytes as 16 lowercase
/// hex digits. A safe name never holds `+`, so the two forms cannot meet, and two plain names are
/// two different branches, so they cannot collide. Only an unsafe or very long name relies on the
/// hash, which keeps `feat/login` apart from `feat-login`. A folder name is at most
/// 7 + 200 + 17 = 224 bytes, under NAME_MAX 255.
pub fn branch_folder(branch: &str) -> Option<String> {
    if matches!(branch, "" | "main") {
        return None;
    }
    let safe = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-');
    let bytes = branch.as_bytes();
    if bytes.len() <= 200
        && bytes.iter().all(|&byte| safe(byte))
        && !matches!(bytes[0], b'.' | b'-')
    {
        return Some(format!(".swarm-{branch}"));
    }
    let slug: String = bytes[..bytes.len().min(200)]
        .iter()
        .map(|&byte| if safe(byte) { byte as char } else { '-' })
        .collect();
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    Some(format!(".swarm-{slug}+{hash:016x}"))
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
        ("feat/login", Some(".swarm-feat-login+407712bf7898fb7f")),
        ("feat-login", Some(".swarm-feat-login")),
        ("feat/👩‍💻", Some(".swarm-feat------------+351989ced13b5f34")),
        ("..", Some(".swarm-..+07da1a07b4a03f2d")),
        ("-x", Some(".swarm--x+07d04207b4982ea0")),
        ("\"main\"", Some(".swarm--main-+f2c462bd1704f4de")),
        ("a'b", Some(".swarm-a-b+e63cb31904812ee9")),
    ];

    /// Long names, shared with `ui/Tests/SwarmCoreTests/SwarmHomeTests.swift` like `VECTORS`.
    fn long_vectors() -> Vec<(String, Option<String>)> {
        let a = |count: usize| "a".repeat(count);
        vec![
            (
                format!("feat/{}", a(235)),
                Some(format!(".swarm-feat-{}+92be3c58bd6b9ccb", a(195))),
            ),
            (
                format!("{}e6uomhlyrr3q", a(64)),
                Some(format!(".swarm-{}e6uomhlyrr3q", a(64))),
            ),
            (
                format!("{}zimprpqj6tk7", a(64)),
                Some(format!(".swarm-{}zimprpqj6tk7", a(64))),
            ),
            (a(200), Some(format!(".swarm-{}", a(200)))),
            (a(201), Some(format!(".swarm-{}+9a253eda0ce95884", a(200)))),
        ]
    }

    #[test]
    fn branch_folders_match_the_shared_vectors() {
        let short = VECTORS.map(|(branch, folder)| (branch.to_string(), folder.map(String::from)));
        for (branch, folder) in short.into_iter().chain(long_vectors()) {
            let actual = branch_folder(&branch);
            assert_eq!(actual, folder, "branch {branch:?}");
            assert!(
                actual.map_or(0, |name| name.len()) <= 255,
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
