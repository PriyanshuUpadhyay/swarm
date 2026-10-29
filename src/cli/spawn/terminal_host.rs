//! Which terminal a chat's chair runs on (ADR 0131).

/// ADR 0131: a chat runs on a PTY host unless the owner chose tmux and tmux is on PATH.
pub(super) fn chat_on_pty(root: &std::path::Path) -> bool {
    let tmux_chosen = swarm::view::choices_store::load(root, true)
        .is_ok_and(|saved| saved.owner.prefs["terminalHost"] == "tmux");
    !(tmux_chosen && tmux_on_path())
}

pub(crate) fn tmux_on_path() -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(dir.join("tmux"))
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    })
}
