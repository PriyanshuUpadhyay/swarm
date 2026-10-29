//! Which terminal a chat's chair runs on (ADR 0131).

/// ADR 0131: a chat runs on a PTY host unless the owner chose tmux and tmux is on PATH.
pub(super) fn chat_on_pty(root: &std::path::Path) -> bool {
    // A choices file that cannot be read, or is locked too long, gives the default host, so a
    // launch never fails on a setting.
    let tmux_chosen = swarm::view::choices_store::load(root, true)
        .is_ok_and(|saved| saved.owner.prefs["terminalHost"] == "tmux");
    !(tmux_chosen && swarm::paths::on_path("tmux"))
}
