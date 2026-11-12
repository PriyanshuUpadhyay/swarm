//! The `reference-transaction` hook that stops an agent shell from moving the default branch
//! (ADR 0135).

use super::{AGENT_MARKERS, Hub, git};

/// The first line of the ref hook swarm writes, so it never replaces another tool's hook.
const HOOK_MARK: &str = "# swarm task guard (ADR 0135)";

/// Write the ref hook into the hub's hooks folder. Returns why it was not written, when it was not.
pub fn install_hook(hub: &Hub) -> Option<String> {
    if let Some(path) = git(&hub.common, &["config", "--get", "core.hooksPath"]) {
        return Some(format!(
            "core.hooksPath is {path}, so the ref hook is not set; the guard wall still blocks agent moves of {}",
            hub.default
        ));
    }
    // The name goes into a shell script, so only plain branch names are written there.
    if !hub
        .default
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
    {
        return Some(format!(
            "the default branch {:?} has characters a hook script would read as shell, so the ref hook is not set",
            hub.default
        ));
    }
    let file = hub.common.join("hooks").join("reference-transaction");
    // Read as bytes, so a hook that is not UTF-8 still counts as another tool's.
    if let Ok(bytes) = std::fs::read(&file)
        && !String::from_utf8_lossy(&bytes).contains(HOOK_MARK)
    {
        return Some(format!(
            "{} is another tool's hook, so swarm's ref hook is not set",
            file.display()
        ));
    }
    let markers = AGENT_MARKERS
        .iter()
        .map(|name| format!("${{{name}:-}}"))
        .collect::<String>();
    let script = format!(
        "#!/bin/sh\n{HOOK_MARK}: an agent shell cannot move {default}; `swarm task land` asks swarm to.\n\
         [ \"$1\" = prepared ] || {{ cat >/dev/null; exit 0; }}\n\
         refused=0\n\
         while read -r old new ref; do\n  [ \"$ref\" = refs/heads/{default} ] && refused=1\ndone\n\
         [ \"$refused\" = 1 ] && [ -n \"{markers}\" ] || exit 0\n\
         echo \"swarm: agents cannot move {default}; edit in a task worktree and run swarm task land\" >&2\n\
         exit 1\n",
        default = hub.default
    );
    let written = std::fs::create_dir_all(file.parent()?)
        .and_then(|()| std::fs::write(&file, script))
        .and_then(|()| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
        });
    written
        .err()
        .map(|error| format!("cannot write {}: {error}", file.display()))
}
