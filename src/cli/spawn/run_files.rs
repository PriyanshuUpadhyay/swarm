//! The files a spawn writes in `runs/<session>/`: the pane's `swarm` link and its run script.

use super::*;

/// Point `runs/<session>/bin/swarm` at `exe` and return that dir.
pub(crate) fn swarm_bin(
    root: &std::path::Path,
    session_id: &str,
    exe: &std::path::Path,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use std::os::unix::fs::DirBuilderExt;
    valid_session_id(session_id)?;
    let dir = root.join(format!("runs/{session_id}/bin"));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    // A new link renamed over the old one, so a pane starting now never finds no `swarm`.
    let tmp = dir.join(format!(".swarm.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    // Homebrew deletes `Cellar/swarm/<version>` on upgrade, so the link names `opt/swarm`, which
    // moves to each new version, and a pane keeps a working `swarm` across an install.
    std::os::unix::fs::symlink(swarm::hud::stable_same(exe), &tmp)?;
    std::fs::rename(&tmp, dir.join("swarm"))?;
    Ok(dir)
}

/// Save `command` as `runs/<session>/<agent>.sh` and return the short line that sources it.
/// A ring types into a new pane before its shell is ready, and the terminal then keeps only the
/// first 1024 bytes of the line, so a long argv never reaches the shell whole. The pane's own
/// shell sources the file, so a `swarm spawn` command can be one of its functions; `env` starts a
/// provider CLI.
pub(crate) fn script_line(
    root: &std::path::Path,
    session_id: &str,
    agent_id: &str,
    command: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    // Both ids become path parts; a `../` in either would write and source a file outside runs/.
    if !swarm::bus::valid_agent_id(agent_id) {
        return Err(format!("swarm: bad agent id {agent_id}").into());
    }
    valid_session_id(session_id)?;
    let dir = root.join(format!("runs/{session_id}"));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    // The line can carry an account's env, so only the owner reads it.
    let path = dir.join(format!("{agent_id}.sh"));
    let tmp = dir.join(format!(".{agent_id}.sh.{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, format!("{command}\n").as_bytes())
        })
        .and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("swarm: cannot write {}: {error}", path.display()).into());
    }
    Ok(format!(
        ". {}",
        swarm::adapter::shell_line(&[path.to_string_lossy().into_owned()])
    ))
}
