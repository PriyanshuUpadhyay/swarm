use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn a_switch_of_a_chat_with_no_adapter_reads_its_chair_on_the_apps_adapter() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = action_fixture();
    // An imported chat has no adapter until its first pane.
    swarm::store::open(&fixture.app.root.join("swarm.db"))
        .unwrap()
        .execute(
            "UPDATE session SET adapter = NULL WHERE id = ?1",
            [&fixture.session],
        )
        .unwrap();
    let cli = &fixture.app.cli;
    std::fs::write(
        cli,
        "#!/bin/sh\nprintf '%s\\n' \"${SWARM_ADAPTER-}|$*\" >> \"$SWARM_HOME/actions\"\n\
         if [ \"$1\" = agents ]; then printf '%s\\n' '{\"agents\":[{\"id\":\"orchestrator\"}]}'; fi\n",
    )
    .unwrap();
    std::fs::set_permissions(cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let body = || -> ContinueChat {
        serde_json::from_value(json!({
            "old_session": fixture.session,
            "provider": "claude",
            "model": "opus",
            "carry": "resume",
        }))
        .unwrap()
    };
    register_continue(&fixture.app, &body()).unwrap();
    // The switch goes on to launch after that read and its result is dropped; only the first read matters here.
    let _ = execute_continue(&fixture.app, body()).await;
    assert_eq!(
        action_calls(&fixture).lines().next(),
        Some("tmux-swarm|agents --json")
    );
}
