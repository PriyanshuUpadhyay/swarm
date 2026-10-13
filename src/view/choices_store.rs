//! File boundary for the pure owner-choice and navigation rules.
use super::choices::{OwnerChoices, ViewState};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct Saved {
    pub owner: OwnerChoices,
    pub state: ViewState,
    pub revision: String,
}
#[derive(Debug)]
pub enum StoreError {
    Busy,
    Stale,
    Failed(String),
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("Another update is in progress. Try again."),
            Self::Stale => f.write_str("choices revision is stale"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

pub fn load(root: &Path, wait: bool) -> Result<Saved, StoreError> {
    if !root.join("swarm-home").is_file() {
        return decode(Vec::new(), Vec::new());
    }
    let _lock = lock(root, wait)?;
    read_unlocked(root)
}

/// Hold the same flock as Swift for the revision test, merge, and atomic replacement.
pub fn update(
    root: &Path,
    revision: Option<&str>,
    change: impl FnOnce(&mut OwnerChoices, &mut ViewState) -> Result<(), String>,
) -> Result<Saved, StoreError> {
    if !root.join("swarm-home").is_file() {
        return Err(StoreError::Failed(format!(
            "Run swarm init with SWARM_HOME set to {}, then try again.",
            root.display()
        )));
    }
    let _lock = lock(root, true)?;
    let mut saved = read_unlocked(root)?;
    if revision.is_some_and(|revision| revision != saved.revision) {
        return Err(StoreError::Stale);
    }
    let previous_owner = saved.owner.clone();
    let previous_state = saved.state.clone();
    change(&mut saved.owner, &mut saved.state).map_err(StoreError::Failed)?;
    // Replace only changed files. Shared choices and web view state have separate owners.
    if saved.owner != previous_owner {
        atomic(root, "choices.json", &saved.owner)?;
    }
    if saved.state != previous_state {
        atomic(root, "view-state.json", &saved.state)?;
    }
    read_unlocked(root)
}

fn optional(path: &Path) -> Result<Vec<u8>, StoreError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
fn read_unlocked(root: &Path) -> Result<Saved, StoreError> {
    let choices = optional(&root.join("choices.json"))?;
    let state = optional(&root.join("view-state.json"))?;
    if !choices.is_empty() && serde_json::from_slice::<OwnerChoices>(&choices).is_err() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut suffix = 0;
        loop {
            let backup = root.join(format!(
                "choices.json.bad-{timestamp}{}",
                if suffix == 0 {
                    String::new()
                } else {
                    format!("-{suffix}")
                }
            ));
            if !backup.exists() {
                std::fs::rename(root.join("choices.json"), backup)?;
                break;
            }
            suffix += 1;
        }
        return decode(Vec::new(), state);
    }
    decode(choices, state)
}
fn decode(choices: Vec<u8>, state: Vec<u8>) -> Result<Saved, StoreError> {
    let owner = if choices.is_empty() {
        OwnerChoices::default()
    } else {
        serde_json::from_slice(&choices).map_err(|e| StoreError::Failed(e.to_string()))?
    };
    let view = if state.is_empty() {
        ViewState::default()
    } else {
        serde_json::from_slice(&state).map_err(|e| StoreError::Failed(e.to_string()))?
    };
    let mut hash = Sha256::new();
    hash.update((choices.len() as u64).to_le_bytes());
    hash.update(&choices);
    hash.update(&state);
    Ok(Saved {
        owner,
        state: view,
        revision: format!("{:x}", hash.finalize()),
    })
}
fn lock(root: &Path, wait: bool) -> Result<File, StoreError> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(root.join("choices.lock"))?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut delay = Duration::from_millis(10);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if wait && Instant::now() < deadline => {
                std::thread::sleep(delay.min(deadline.saturating_duration_since(Instant::now())));
                delay = (delay * 2).min(Duration::from_millis(100));
            }
            Err(std::fs::TryLockError::WouldBlock) => return Err(StoreError::Busy),
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}
fn atomic(root: &Path, name: &str, value: &impl Serialize) -> Result<(), StoreError> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let path: PathBuf = root.join(format!(".{name}.{}", uuid::Uuid::now_v7()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&path)?;
        let bytes =
            serde_json::to_vec_pretty(value).map_err(|e| StoreError::Failed(e.to_string()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&path, root.join(name))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result
}
