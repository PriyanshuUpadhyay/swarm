use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn commands(home: &Path, project: &Path) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .current_dir(project)
        .args(["commands", "--provider", "claude", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn a_saved_claude_list_is_used_until_a_skill_file_changes() {
    let home = std::env::temp_dir().join(format!("swarm-commands-saved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let project = home.join("project");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::create_dir_all(home.join("bin")).unwrap();
    let claude = home.join("bin/claude");
    std::fs::write(
        &claude,
        format!(
            r#"#!/bin/sh
echo started >> "{}/starts"
read -r _
echo '{{"type":"control_response","response":{{"request_id":"swarm-commands","subtype":"success","response":{{"commands":[{{"name":"plan","description":"Plan"}}]}}}}}}'
while read -r _; do :; done
"#,
            home.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let starts = || {
        std::fs::read_to_string(home.join("starts"))
            .unwrap()
            .lines()
            .count()
    };

    let first = commands(&home, &project);
    assert!(first["commands"].to_string().contains("/plan"));
    assert_eq!(commands(&home, &project), first);
    assert_eq!(starts(), 1);

    let skill = project.join(".claude/skills/new");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: new\ndescription: New\n---\n",
    )
    .unwrap();
    commands(&home, &project);
    assert_eq!(starts(), 2);
}
