// Model usage and spent account tests.

use super::*;

fn model_cache(
    home: &Path,
    provider: &str,
    work_fb: serde_json::Value,
    default_fb: serde_json::Value,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let accounts: Vec<_> = [("work", 10, work_fb), ("default", 50, default_fb)].into_iter().map(|(name, used, fb)| {
        let dir = match (provider, name) { ("claude", "work") => ".claude/.profiles/work", ("claude", _) => ".claude", ("codex", "work") => ".codex-work", _ => ".codex" };
        let mut meters = vec![serde_json::json!({"provider":provider,"account":name,"label":name,"window":"5h","used_pct":used,"state":"fresh","as_of_seconds":now})];
        if !fb.is_null() {
            meters.push(serde_json::json!({"provider":provider,"account":name,"label":name,"window":"fb","used_pct":fb,"state":"fresh","reset_time_seconds":now+3600,"as_of_seconds":now}));
        }
        serde_json::json!({"home":home.join(dir),"meters":meters})
    }).collect();
    std::fs::write(
        home.join(format!(".swarm/{provider}-usage.json")),
        serde_json::json!({"accounts":accounts}).to_string(),
    )
    .unwrap();
}

#[test]
fn run_and_account_json_scope_fable_usage_to_the_model() {
    let home = fixture("model-scope");
    model_cache(&home, "claude", 100.into(), 10.into());
    for (args, expected) in [
        (vec!["run", "claude"], "work"),
        (vec!["run", "claude", "--", "--model", "sonnet"], "work"),
        (
            vec!["run", "claude", "--", "--model", "FABLE[1m]"],
            "default",
        ),
        (
            vec!["run", "claude", "--", "--model=claude-fable-5-1"],
            "default",
        ),
        (vec!["run", "claude", "--", "--model", "best"], "default"),
        (
            vec![
                "run",
                "claude",
                "--account",
                "work",
                "--",
                "--model",
                "fable",
            ],
            "work",
        ),
    ] {
        let output = command(&home, &args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with(&format!("account {expected}\n")),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = command(&home, &["accounts", "--provider", "claude", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(list["auto"], "work");
    assert_eq!(list["accounts"][2]["remaining_pct"], 90);
    model_cache(&home, "codex", 100.into(), 100.into());
    let output = command(&home, &["run", "codex", "--", "--model", "fable"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "account default\n");
}

#[test]
fn run_refuses_all_spent_fable_accounts_and_keeps_unknown_samples_usable() {
    let home = fixture("model-all-spent");
    for (fb, refused) in [
        (serde_json::json!(100), true),
        (serde_json::Value::Null, false),
        (serde_json::json!(101), false),
    ] {
        model_cache(&home, "claude", fb.clone(), fb);
        let output = command(&home, &["run", "claude", "--", "--model", "best"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            !refused,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if refused {
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("every signed-in Claude account has used its Fable limit")
            );
            assert!(!home.join("child-env").exists());
        }
    }
}
