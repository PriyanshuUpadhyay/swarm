//! A Codex async question, read from its log and answered with a typed reply.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use swarm::screen::agent_prompt;

const IDLE: &str = include_str!("fixtures/screens/codex-idle.txt");
// Codex 0.162.1 shows an async question as a collapsed `? 1 question` row, so only its log
// names the question and its choices.
const CALL: &str = r#"{"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","arguments":"{\"questions\":[{\"title\":\"Sign in now?\",\"options\":[\"Yes\",\"Later\"]}]}","call_id":"call-a"}}"#;
const REPLY: &str = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<send_user_message_question_reply>[{\"answer\":\"Later\",\"question\":\"Sign in now?\",\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"call-a\\\",0]\"}]</send_user_message_question_reply>"}]}}"#;

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!("swarm-codex-question-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn codex_async_question_reads_from_the_log_until_its_reply_or_turn_end() {
    let ended = r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#;
    let root = scratch();
    let log = root.join("rollout.jsonl");
    let read = |lines: &[&str]| {
        std::fs::write(&log, lines.join("\n") + "\n").unwrap();
        agent_prompt(Some("codex"), IDLE, || None, Some(&log))
    };
    let found = read(&[CALL]).expect("the open async question is a prompt");
    assert_eq!(found.question, "Sign in now?");
    assert_eq!(found.choices, ["Yes", "Later"]);
    assert_eq!(
        found.reply.as_deref(),
        Some(r#"["request_user_input_async","call-a",0]"#)
    );
    assert_eq!(read(&[CALL, REPLY]), None);
    // A call with two questions shows the first one first, as the Codex TUI does.
    let two = CALL.replace(
        r#"]}]}"#,
        r#"]},{\"title\":\"Then what?\",\"options\":[\"Stop\"]}]}"#,
    );
    assert_eq!(read(&[&two]).unwrap().question, "Sign in now?");
    assert_eq!(read(&[&two, REPLY]).unwrap().question, "Then what?");
    assert_eq!(read(&[CALL, ended]), None);
    // Codex keeps working after it asks; a real log held 9 MB of tool output before the reply.
    let output = format!(
        r#"{{"type":"response_item","payload":{{"type":"function_call_output","call_id":"x","output":"{}"}}}}"#,
        "x".repeat(1024 * 1024)
    );
    let started = r#"{"type":"event_msg","payload":{"type":"task_started"}}"#;
    let busy: Vec<&str> = [started, CALL]
        .into_iter()
        .chain(std::iter::repeat_n(output.as_str(), 3))
        .collect();
    assert_eq!(read(&busy).unwrap().question, "Sign in now?");
    std::fs::remove_dir_all(root).unwrap();
}

fn cli(home: &Path, session: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .env_clear()
        .env("HOME", home)
        .env("SWARM_HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SWARM_ADAPTER", "fake")
        .env("SWARM_SESSION_ID", session)
        .env("SWARM_AGENT_ID", "orchestrator")
        .current_dir(home)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn answer_types_the_reply_only_when_the_codex_input_box_shows() {
    let home = scratch();
    let root = home.join(".swarm");
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    std::fs::write(root.join("swarm-home"), "swarm\n").unwrap();
    // A redraw with no input box, where typed text would go nowhere or into another view. The
    // tmux capture holds the whole scrollback, so an old `›` history row must not count.
    let frame = format!(
        "› Run the tests\n{}• Working (3s • esc to interrupt)\n",
        "• Ran cargo test\n".repeat(20)
    );
    std::fs::write(home.join("screen"), frame).unwrap();
    let log = home.join("rollout.jsonl");
    std::fs::write(&log, format!("{CALL}\n")).unwrap();
    std::fs::write(root.join("adapters/fake.conf"), "self = true\nspawn = true\nring = printf '%s' \"$SWARM_TEXT\" >> \"$HOME/typed\"\nlist = printf pane\nclose = true\ncapture = cat \"$HOME/screen\"\nscreen = cat \"$HOME/screen\"\nkey = printf '%s' \"$SWARM_KEY\" >> \"$HOME/keys\"\n").unwrap();
    let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
    let session =
        swarm::store::create_session(&connection, "lane", &home, None, Some("fake")).unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "code").unwrap();
    swarm::store::set_pane(&connection, &session, "coder", "pane").unwrap();
    swarm::store::set_provider(&connection, &session, "coder", "codex").unwrap();
    swarm::store::set_log(&connection, &session, "coder", &log).unwrap();
    let id = agent_prompt(Some("codex"), IDLE, || None, Some(&log))
        .unwrap()
        .id;

    let refused = cli(&home, &session, &["answer", "coder", &id, "1"]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("input box"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!home.join("typed").exists());

    std::fs::write(home.join("screen"), IDLE).unwrap();
    let output = cli(&home, &session, &["answer", "coder", &id, "1"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(home.join("typed")).unwrap(),
        r#"<send_user_message_question_reply>[{"answer":"Later","question":"Sign in now?","questionItemId":"[\"request_user_input_async\",\"call-a\",0]"}]</send_user_message_question_reply>"#
    );
    assert!(!home.join("keys").exists());

    // A Herdr pane's `screen` verb prints status JSON, so the box check reads the pane rows.
    std::fs::remove_file(home.join("typed")).unwrap();
    std::fs::write(home.join("rows"), IDLE).unwrap();
    std::fs::write(root.join("adapters/fake.conf"), "self = true\nspawn = true\nring = printf '%s' \"$SWARM_TEXT\" >> \"$HOME/typed\"\nlist = printf pane\nclose = true\ncapture = cat \"$HOME/rows\"\nscreen = printf '{\"result\":{\"agent\":{\"agent_status\":\"idle\"}}}'\nkey = printf '%s' \"$SWARM_KEY\" >> \"$HOME/keys\"\n").unwrap();
    let output = cli(&home, &session, &["answer", "coder", &id, "1"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.join("typed").exists());
    std::fs::remove_dir_all(home).unwrap();
}
