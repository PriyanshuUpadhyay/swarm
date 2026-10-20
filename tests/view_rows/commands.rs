// Tests for commands.

use super::*;

#[test]
fn shell_pair() {
    let rows = build(vec![
        shell_input("ls", "input"),
        shell_output(
            "<bash-stdout>a &amp; b</bash-stdout><bash-stderr></bash-stderr>",
            "input",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::Shell]);
    assert_eq!(rows[0].id, "input:shell");
    assert_eq!(
        rows[0].shell.as_ref().unwrap().command.as_deref(),
        Some("ls")
    );
    assert_eq!(rows[0].shell.as_ref().unwrap().output.text, "a & b");
    assert_eq!(rows[0].text, "$ ls\na & b");
}
#[test]
fn unmatched_shell() {
    let lone = build(vec![shell_input("pwd", "input")]);
    let orphan = build(vec![shell_output(
        "<bash-stdout>/work</bash-stdout>",
        "outside-window",
    )]);
    assert_eq!(
        lone[0].shell.as_ref().unwrap().command.as_deref(),
        Some("pwd")
    );
    assert_eq!(lone[0].shell.as_ref().unwrap().output.text, "");
    assert!(orphan[0].shell.as_ref().unwrap().command.is_none());
    assert_eq!(orphan[0].shell.as_ref().unwrap().output.text, "/work");
    assert!(lone[0].starts_turn && orphan[0].starts_turn);
}
#[test]
fn duplicate_shell_inputs() {
    let rows = build(vec![
        shell_input("first", "same"),
        shell_input("second", "same"),
        shell_output("<bash-stdout>which one</bash-stdout>", "same"),
    ]);
    assert_eq!(kinds(&rows), [RowKind::Shell; 3]);
    assert_eq!(
        rows.iter()
            .map(|row| row.shell.as_ref().unwrap().command.as_deref())
            .collect::<Vec<_>>(),
        [Some("first"), Some("second"), None]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.shell.as_ref().unwrap().output.text.as_str())
            .collect::<Vec<_>>(),
        ["", "", "which one"]
    );
}
#[test]
fn cut_command_has_no_chip() {
    let rows = build(vec![
        system("command", "<command-name>/flo", "command", ""),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "body",
            "command",
        ),
    ]);
    assert!(rows.iter().all(|row| row.command.is_none()));
    assert_eq!(
        texts(&rows),
        [
            "<command-name>/flo",
            "Base directory for this skill: /skills/flow"
        ]
    );
}
#[test]
fn pi_skill_message_to_command() {
    // The shape pi 1.2.0 writes for `/skill:plan fix this now`.
    let line = r#"{"type":"message","id":"u1","parentId":"a0","timestamp":"t","message":{"role":"user","content":[{"type":"text","text":"<skill name=\"plan\" location=\"/s/plan/SKILL.md\">\nReferences are relative to /s/plan.\n\n# Plan\n</skill>\n\nfix this now"}]}}"#;
    let rows = build(inline_fixture("pi", &[line]));
    assert_eq!(kinds(&rows), [RowKind::System]);
    let chip = rows[0].command.as_ref().unwrap();
    assert_eq!(chip.name, "/skill:plan");
    assert_eq!(chip.arguments, "fix this now");
    assert_eq!(
        chip.skill_body.as_ref().unwrap().text,
        "References are relative to /s/plan.\n\n# Plan"
    );
    assert!(rows[0].starts_turn);
}
#[test]
fn skill_body_to_command() {
    let command = "<command-message>flow</command-message>\n<command-name>/flow</command-name>\n<command-args>start\nthe chat</command-args>";
    let rows = build(vec![
        system("command", command, "command", ""),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "body",
            "command",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::System]);
    assert_eq!(rows[0].text, command);
    assert_eq!(rows[0].system_kind.as_deref(), Some("command"));
    let chip = rows[0].command.as_ref().unwrap();
    assert_eq!(chip.name, "/flow");
    assert_eq!(chip.arguments, "start\nthe chat");
    assert_eq!(
        chip.skill_body.as_ref().unwrap().text,
        "Base directory for this skill: /skills/flow"
    );
    assert!(rows[0].starts_turn);
}
#[test]
fn skill_body_to_tool() {
    let rows = build(vec![
        call("skill-call", "Skill", json!({"skill":"flow"})),
        update("skill-call", "completed", "Launching skill: flow"),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "skill-call",
        ),
    ]);
    assert_eq!(kinds(&rows), [RowKind::ToolUse]);
    assert_eq!(
        tool(&rows[0]).skill_body.as_ref().unwrap().text,
        "Base directory for this skill: /skills/flow"
    );
    let twice = build(vec![
        call("skill-call", "Skill", json!({"skill":"flow"})),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "skill-call",
        ),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/flow/more",
            "skill-call",
        ),
    ]);
    assert_eq!(
        tool(&twice[0]).skill_body.as_ref().unwrap().text,
        "Base directory for this skill: /skills/flow\n\nBase directory for this skill: /skills/flow/more"
    );
}
#[test]
fn bundled_skill_body_to_tool() {
    let rows = build(vec![
        call("skill-call", "Skill", json!({"skill":"simplify"})),
        skill_body("injected", "Review target: the changes", "skill-call"),
        call("bash-call", "Bash", json!({})),
        skill_body("injected", "Hook note", "bash-call"),
    ]);
    assert_eq!(
        kinds(&rows),
        [RowKind::ToolUse, RowKind::ToolUse, RowKind::System]
    );
    assert_eq!(
        tool(&rows[0]).skill_body.as_ref().unwrap().text,
        "Review target: the changes"
    );
    assert!(tool(&rows[1]).skill_body.is_none());
    assert!(rows[2].collapsed);
}
#[test]
fn unlinked_skill_body() {
    let rows = build(vec![system(
        "skill_body",
        "Base directory for this skill: /skills/flow",
        "body",
        "outside-window",
    )]);
    assert_eq!(kinds(&rows), [RowKind::System]);
    assert_eq!(rows[0].system_kind.as_deref(), Some("skill_body"));
    assert!(!rows[0].collapsed);
}
#[test]
fn model_command_output() {
    let rows = build(vec![
        system(
            "command",
            "<command-name>/model</command-name>\n<command-args>sonnet</command-args>",
            "command",
            "",
        ),
        system(
            "command_output",
            "<local-command-stdout>Set model to \u{1b}[1msonnet\u{1b}[22m</local-command-stdout>",
            "stdout",
            "command",
        ),
    ]);
    assert_eq!(rows.len(), 1);
    let chip = rows[0].command.as_ref().unwrap();
    assert_eq!(chip.name, "/model");
    assert_eq!(chip.arguments, "sonnet");
    assert_eq!(chip.output.as_ref().unwrap().text, "Set model to sonnet");
    assert!(!rows[0].starts_turn);
    assert!(!transcript::is_active(&rows));
}
#[test]
fn bundled_skill_body_to_command() {
    let rows = build(vec![
        user("Fix the bug", "prompt"),
        ended("completed", Some(1)),
        system(
            "command",
            "<command-name>/simplify</command-name>",
            "command",
            "",
        ),
        system("injected", "Review target: the changes", "body", "command"),
    ]);
    assert_eq!(
        kinds(&rows),
        [RowKind::User, RowKind::Result, RowKind::System]
    );
    assert_eq!(
        rows[2]
            .command
            .as_ref()
            .unwrap()
            .skill_body
            .as_ref()
            .unwrap()
            .text,
        "Review target: the changes"
    );
    assert!(transcript::is_active(&rows));
}
#[test]
fn reminder_under_command_stays_hidden() {
    for reminder in [
        "\n<system-reminder>Be brief</system-reminder>",
        "<local-command-caveat>Caveat: do not respond</local-command-caveat>",
    ] {
        let rows = build(vec![
            system(
                "command",
                "<command-name>/simplify</command-name>",
                "command",
                "",
            ),
            system("injected", reminder, "reminder", "command"),
        ]);
        assert_eq!(kinds(&rows), [RowKind::System; 2]);
        assert!(rows[0].command.as_ref().unwrap().skill_body.is_none());
        assert!(!rows[0].starts_turn);
        assert!(rows[1].collapsed);
    }
}
#[test]
fn two_skill_bodies_join() {
    let rows = build(vec![
        system(
            "command",
            "<command-name>/flow</command-name>",
            "command",
            "",
        ),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "first-body",
            "command",
        ),
        system(
            "injected",
            "Review target: the changes",
            "second-body",
            "command",
        ),
    ]);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]
            .command
            .as_ref()
            .unwrap()
            .skill_body
            .as_ref()
            .unwrap()
            .text,
        "Base directory for this skill: /skills/flow\n\nReview target: the changes"
    );
}
#[test]
fn search_text_holds_joined_records() {
    let rows = build(vec![
        system(
            "command",
            "<command-name>/model</command-name>",
            "model-command",
            "",
        ),
        system(
            "command_output",
            "<local-command-stdout>Set model to sonnet</local-command-stdout>",
            "stdout",
            "model-command",
        ),
        system(
            "command",
            "<command-name>/flow</command-name>",
            "flow-command",
            "",
        ),
        system(
            "skill_body",
            "Base directory for this skill: /skills/flow",
            "command-body",
            "flow-command",
        ),
        call("skill-call", "Skill", json!({"skill":"review"})),
        skill_body(
            "skill_body",
            "Base directory for this skill: /skills/review",
            "skill-call",
        ),
    ]);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].search_text().contains("Set model to sonnet"));
    assert!(rows[1].search_text().contains("/skills/flow"));
    assert!(rows[2].search_text().contains("/skills/review"));
}
