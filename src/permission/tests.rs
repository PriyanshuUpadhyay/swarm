use super::*;

fn payload() -> Value {
    json!({"hook_event_name":"PermissionRequest", "tool_name":"Bash",
    "tool_input":{"command":"touch /tmp/work/probe.txt"},
    "permission_suggestions":[
        {"type":"setMode", "mode":"acceptEdits", "destination":"session"},
        {"type":"addDirectories", "directories":["/tmp/work"], "destination":"session"},
        {"type":"addRules", "rules":[{"toolName":"Bash", "ruleContent":"touch:*"}], "behavior":"allow", "destination":"localSettings"}
    ]})
}

#[test]
fn screen_choices_match_suggestion_types_instead_of_array_positions() {
    let p = payload();
    for (label, index) in [
        (
            "Yes, and always allow access to\n/tmp/work from this project",
            1,
        ),
        ("Yes, allow all edits during this session", 0),
        ("Yes, and don't ask again", 2),
        ("Yes, and don't ask again for: touch:*", 2),
    ] {
        assert_eq!(
            decision(&p, label),
            Some(
                json!({"behavior":"allow", "updatedPermissions":[p["permission_suggestions"][index]]})
            ),
            "{label}"
        );
    }
    assert_eq!(decision(&p, "Yes"), Some(json!({"behavior":"allow"})));
    assert_eq!(decision(&p, "No").unwrap()["behavior"], "deny");
    for label in [
        "Yes, and switch to auto mode · auto mode handles these prompts for you",
        "Yes, and always allow access to /tmp/work-more from this project",
        "Yes, and don't ask again for: rm:*",
        "Trust this folder",
    ] {
        assert_eq!(decision(&p, label), None, "{label}");
    }
}

#[test]
fn missing_or_ambiguous_suggestions_use_the_key_path() {
    let mut p = payload();
    let duplicate = p["permission_suggestions"][2].clone();
    p["permission_suggestions"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert_eq!(decision(&p, "Yes, and don't ask again"), None);
    p["permission_suggestions"] = json!([]);
    assert_eq!(decision(&p, "Yes, and don't ask again"), None);
    assert_eq!(decision(&p, "Yes"), Some(json!({"behavior":"allow"})));
}

#[test]
fn a_request_must_match_the_current_tool_and_permission_screen() {
    let screen = include_str!("../../tests/fixtures/screens/claude-waiting.txt");
    let mut prompt = crate::screen::prompt(screen).unwrap();
    let mut p = payload();
    assert!(matches_prompt(&p, &prompt));
    p["tool_input"]["command"] = json!("touch /tmp/different.txt");
    assert!(!matches_prompt(&p, &prompt));
    p = payload();
    prompt.question = "Do you trust this folder?".into();
    assert!(!matches_prompt(&p, &prompt));
    p["tool_name"] = json!("AskUserQuestion");
    assert!(!is_request(&p));
    p["tool_name"] = json!("ExitPlanMode");
    assert!(!is_request(&p));
}

#[test]
fn the_exclusive_file_claim_accepts_one_answer_across_threads() {
    let root = std::env::temp_dir().join(format!("swarm-permission-race-{}", uuid::Uuid::now_v7()));
    let session = uuid::Uuid::now_v7().to_string();
    let id = add_permission_request(&root, &session, "worker", &payload(), 100).unwrap();
    let barrier = std::sync::Barrier::new(3);
    let (allow, deny) = std::thread::scope(|scope| {
        let allow = scope.spawn(|| {
            barrier.wait();
            answer_permission_request(
                &root,
                &session,
                "worker",
                &id,
                Some(&json!({"behavior":"allow"})),
                1,
            )
            .is_ok()
        });
        let deny = scope.spawn(|| {
            barrier.wait();
            answer_permission_request(
                &root,
                &session,
                "worker",
                &id,
                Some(&json!({"behavior":"deny"})),
                1,
            )
            .is_ok()
        });
        barrier.wait();
        (allow.join().unwrap(), deny.join().unwrap())
    });
    assert_ne!(allow, deny);
    match poll_permission_request(&root, &session, &id, 2).unwrap() {
        PermissionPoll::Decision(decision) => {
            assert_eq!(decision["behavior"], if allow { "allow" } else { "deny" })
        }
        _ => panic!("the winning answer was not published"),
    }
    assert!(matches!(
        poll_permission_request(&root, &session, &id, 2).unwrap(),
        PermissionPoll::Closed
    ));
    std::fs::remove_dir_all(root).unwrap();
}
