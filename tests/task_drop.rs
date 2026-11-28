use std::process::Command;

/// Guard: `drop --force` skips the check for work that main lacks, so an agent's shell must
/// not run it (ADR 0173).
#[test]
fn an_agent_shell_cannot_force_a_drop() {
    let home = std::env::temp_dir().join(format!("swarm-task-drop-{}", uuid::Uuid::now_v7()));
    let root = home.join(".swarm");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("swarm-home"), "swarm\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .current_dir(&home)
        .env("SWARM_HOME", &home)
        .env("SWARM_AGENT_ID", "coder")
        .args(["task", "drop", "fix-x", "--force"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&home).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("ask the owner to run `swarm task drop fix-x --force`"),
        "{stderr}"
    );
}
