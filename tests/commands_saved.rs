use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn commands(home: &Path, project: &Path, provider: &str) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join("bin").display()),
        )
        .current_dir(project)
        .args(["commands", "--provider", provider, "--json"])
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
fn a_saved_list_is_used_until_a_skill_file_changes() {
    let claude = r#"{"type":"control_response","response":{"request_id":"swarm-commands","subtype":"success","response":{"commands":[{"name":"plan","description":"Plan"}]}}}"#;
    let pi = r#"{"type":"response","command":"get_commands","success":true,"data":{"commands":[{"name":"plan","source":"prompt"}]}}"#;
    for (provider, reply, skills) in [
        ("claude", claude, ".claude/skills"),
        ("pi", pi, ".agents/skills"),
    ] {
        let home = std::env::temp_dir().join(format!(
            "swarm-commands-saved-{provider}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let project = home.join("project");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(home.join("bin")).unwrap();
        let cli = home.join("bin").join(provider);
        let starts = home.join("starts");
        std::fs::write(
            &cli,
            format!(
                "#!/bin/sh\necho started >> '{}'\nread -r _\necho '{reply}'\nwhile read -r _; do :; done\n",
                starts.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let count = || std::fs::read_to_string(&starts).unwrap().lines().count();

        let first = commands(&home, &project, provider);
        assert!(
            first["commands"].to_string().contains("/plan"),
            "{provider}"
        );
        assert_eq!(commands(&home, &project, provider), first, "{provider}");
        assert_eq!(count(), 1, "{provider}");

        let skill = project.join(skills).join("new");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: new\ndescription: New\n---\n",
        )
        .unwrap();
        commands(&home, &project, provider);
        assert_eq!(count(), 2, "{provider}");
    }
}

#[test]
fn a_saved_pi_model_list_is_used_until_pi_s_models_file_changes() {
    let home = std::env::temp_dir().join(format!("swarm-models-saved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::create_dir_all(home.join(".pi/agent")).unwrap();
    let starts = home.join("starts");
    let pi = home.join("bin/pi");
    std::fs::write(
        &pi,
        format!(
            "#!/bin/sh\necho started >> '{}'\nprintf 'provider  model  context  max-out  thinking  images\\nacme  acme-1  200K  8K  yes  no\\n'\n",
            starts.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
    let models = || {
        let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .env_clear()
            .env("HOME", &home)
            .env("SWARM_HOME", &home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", home.join("bin").display()),
            )
            .current_dir(&home)
            .args(["models", "--provider", "pi", "--json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let count = || std::fs::read_to_string(&starts).unwrap().lines().count();

    let first = models();
    assert!(first["models"].to_string().contains("acme-1"));
    assert_eq!(models(), first);
    assert_eq!(count(), 1);
    std::fs::write(home.join(".pi/agent/models.json"), "{}").unwrap();
    models();
    assert_eq!(count(), 2);
}
