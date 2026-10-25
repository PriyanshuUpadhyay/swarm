//! Each request owns one short-lived folder. An exclusive claim chooses exactly one answer.

use super::*;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

type Error = Box<dyn std::error::Error>;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PermissionRequest {
    pub id: String,
    pub agent: String,
    pub payload: Value,
    pub expires_at_ms: i64,
}

pub enum PermissionPoll {
    Waiting,
    Decision(Value),
    Closed,
}

fn uuid_component(id: &str) -> Result<&str, Error> {
    if uuid::Uuid::parse_str(id)
        .is_ok_and(|uuid| uuid.to_string() == id && uuid.get_version_num() == 7)
    {
        Ok(id)
    } else {
        Err("swarm: bad permission request or session id".into())
    }
}

fn requests_dir(root: &Path, session: &str) -> Result<PathBuf, Error> {
    Ok(root
        .join("runs")
        .join(uuid_component(session)?)
        .join("permission"))
}

fn request_dir(root: &Path, session: &str, id: &str) -> Result<PathBuf, Error> {
    Ok(requests_dir(root, session)?.join(uuid_component(id)?))
}

fn optional_file(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn publish(folder: &Path, name: &str, value: &impl serde::Serialize) -> Result<(), Error> {
    let path = folder.join(format!("{name}.tmp"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    drop(file);
    fs::rename(path, folder.join(name))?;
    Ok(())
}

pub fn add_permission_request(
    root: &Path,
    session: &str,
    agent: &str,
    payload: &Value,
    expires_at_ms: i64,
) -> Result<String, Error> {
    let id = uuid::Uuid::now_v7().to_string();
    let folder = request_dir(root, session, &id)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)?;
    let request = PermissionRequest {
        id: id.clone(),
        agent: agent.to_string(),
        payload: payload.clone(),
        expires_at_ms,
    };
    if let Err(error) = publish(&folder, "request.json", &request) {
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok(id)
}

fn requests(root: &Path, session: &str) -> Result<Vec<PermissionRequest>, Error> {
    let entries = match fs::read_dir(requests_dir(root, session)?) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut requests = Vec::new();
    for entry in entries {
        let entry = entry?;
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {}
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        uuid_component(&id)?;
        let Some(bytes) = optional_file(&entry.path().join("request.json"))? else {
            continue;
        };
        let request: PermissionRequest = serde_json::from_slice(&bytes)?;
        if request.id != id {
            return Err("swarm: permission request identity changed".into());
        }
        requests.push(request);
    }
    // A fresh request outranks the delivered receipt of an identical earlier dialog.
    requests.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(requests)
}

/// A delivered receipt stops a second answer while the old screen is still visible. More than
/// one matching live hook is ambiguous and must not select a hidden request.
pub fn permission_for_prompt(
    root: &Path,
    session: &str,
    agent: &str,
    prompt: &crate::screen::Prompt,
    now_ms: i64,
) -> Result<Option<PermissionRequest>, Error> {
    let mut active = None;
    let mut delivered = None;
    for request in requests(root, session)? {
        if request.expires_at_ms <= now_ms {
            remove_permission_request(root, session, &request.id)?;
            continue;
        }
        if request.agent != agent || !matches_prompt(&request.payload, prompt) {
            continue;
        }
        let folder = request_dir(root, session, &request.id)?;
        if optional_file(&folder.join("delivered.json"))?.is_some() {
            if delivered.is_none() {
                delivered = Some(request);
            }
        } else if active.is_some() {
            return Err(
                "swarm: more than one hook matches this permission; answer it in the terminal"
                    .into(),
            );
        } else {
            active = Some(request);
        }
    }
    Ok(active.or(delivered))
}

fn expired() -> Error {
    "swarm: this permission request expired or was already answered; read it again".into()
}

/// The claim is O_EXCL. The answer appears through rename only after all of its JSON is written.
/// A key-only choice publishes null, so the hook leaves Claude's dialog in control.
pub fn answer_permission_request(
    root: &Path,
    session: &str,
    agent: &str,
    id: &str,
    decision: Option<&Value>,
    now_ms: i64,
) -> Result<(), Error> {
    let folder = request_dir(root, session, id)?;
    let bytes = optional_file(&folder.join("request.json"))?.ok_or_else(expired)?;
    let request: PermissionRequest = serde_json::from_slice(&bytes)?;
    if request.id != id || request.agent != agent || request.expires_at_ms <= now_ms {
        return Err(expired());
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(folder.join("claim"))
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotFound => expired(),
            _ => error.into(),
        })?;
    publish(&folder, "answer.json", &decision)?;
    Ok(())
}

pub fn poll_permission_request(
    root: &Path,
    session: &str,
    id: &str,
    now_ms: i64,
) -> Result<PermissionPoll, Error> {
    let folder = request_dir(root, session, id)?;
    let Some(bytes) = optional_file(&folder.join("request.json"))? else {
        return Ok(PermissionPoll::Closed);
    };
    let request: PermissionRequest = serde_json::from_slice(&bytes)?;
    if request.id != id {
        return Err("swarm: permission request identity changed".into());
    }
    if request.expires_at_ms <= now_ms {
        remove_permission_request(root, session, id)?;
        return Ok(PermissionPoll::Closed);
    }
    if optional_file(&folder.join("delivered.json"))?.is_some() {
        return Ok(PermissionPoll::Closed);
    }
    let Some(bytes) = optional_file(&folder.join("answer.json"))? else {
        return Ok(PermissionPoll::Waiting);
    };
    let decision: Option<Value> = serde_json::from_slice(&bytes)?;
    match fs::rename(folder.join("answer.json"), folder.join("delivered.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PermissionPoll::Closed);
        }
        Err(error) => return Err(error.into()),
    }
    Ok(match decision {
        Some(decision) => PermissionPoll::Decision(decision),
        None => PermissionPoll::Closed,
    })
}

pub fn remove_permission_request(root: &Path, session: &str, id: &str) -> Result<(), Error> {
    let folder = request_dir(root, session, id)?;
    match fs::remove_dir_all(folder) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// A terminal answer does not cancel Claude's running hook. Its completion event ends the wait.
pub fn close_permission_requests(
    root: &Path,
    session: &str,
    agent: &str,
    payload: &Value,
    now_ms: i64,
) -> Result<(), Error> {
    for request in requests(root, session)? {
        let finished = match payload["hook_event_name"].as_str() {
            Some("Stop" | "StopFailure" | "UserPromptSubmit") => true,
            Some("PostToolUse") => {
                request.payload["tool_name"] == payload["tool_name"]
                    && request.payload["tool_input"] == payload["tool_input"]
            }
            _ => false,
        };
        if request.expires_at_ms <= now_ms || (request.agent == agent && finished) {
            remove_permission_request(root, session, &request.id)?;
        }
    }
    Ok(())
}
