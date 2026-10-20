use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use swarm::screen::{agent_prompt, whole_prompt};

const TALL: &str = include_str!("fixtures/screens/claude-question-top-offscreen.txt");
const TWO: &str = include_str!("fixtures/screens/claude-two-questions.txt");
const LOG: &str = include_str!("fixtures/screens/claude-open-question.jsonl");
const QUESTION: &str = "How should we handle the release with open review findings?";

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!("swarm-question-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn tall_question_uses_only_a_matching_open_log_question() {
    let root = scratch();
    let log = root.join("question.jsonl");
    std::fs::write(&log, LOG).unwrap();
    assert_eq!(whole_prompt(TALL, || Some(TALL.into())), None);
    let read = || agent_prompt(Some("claude"), TALL, || Some(TALL.into()), Some(&log));
    let found = read().expect("the open log question fills the missing screen rows");
    assert_eq!(found.question, QUESTION);
    assert_eq!(found.choices.len(), 6);
    assert_eq!(
        found.choices[..4],
        [
            "Release on the planned date",
            "Fix critical findings, then release this week",
            "Fix every finding before any release",
            "Release gradually to a small user group"
        ]
    );
    assert_eq!(found.choices[4..], ["Type something.", "Chat about this"]);
    assert_eq!(
        serde_json::to_value(&found).unwrap()["details"],
        serde_json::json!([
            "Release on the planned date and list the open review findings in the release notes.",
            "Hold the release a few days to repair only the high-severity review findings.",
            "Delay the release until all review findings are fixed and verified by a full retest.",
            "Ship the build to a small share of users first, watch crash and error reports, and widen the rollout in steps.",
            "",
            ""
        ])
    );
    assert!(found.numbered);
    assert!(!found.cut);
    assert_eq!(found.cursor, 0);
    assert_eq!(found.keys(1), Some(vec!["2".into()]));
    assert_eq!(agent_prompt(Some("codex"), TALL, || None, Some(&log)), None);
    assert_eq!(agent_prompt(Some("claude"), TALL, || None, None), None);

    std::fs::write(
        &log,
        LOG.replace(
            "Fix critical findings, then release this week",
            "Other release",
        ),
    )
    .unwrap();
    assert_eq!(read(), None);
    std::fs::write(&log, format!("{LOG}{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"release-choice\",\"content\":\"answered\"}}]}}}}\n")).unwrap();
    assert_eq!(read(), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_keeps_empty_descriptions_plain_and_preserves_description_lines() {
    let root = scratch();
    let log = root.join("question.jsonl");
    let mut record: serde_json::Value = serde_json::from_str(LOG).unwrap();
    let options = record["message"]["content"][0]["input"]["questions"][0]["options"]
        .as_array_mut()
        .unwrap();
    options[0].as_object_mut().unwrap().remove("description");
    options[1]["description"] = "".into();
    options[2]["description"] = " \tFix all findings.\nRun the full retest.\n".into();
    std::fs::write(&log, format!("{record}\n")).unwrap();
    let read = || agent_prompt(Some("claude"), TALL, || None, Some(&log)).unwrap();
    let found = read();
    assert_eq!(found.choices[0], "Release on the planned date");
    assert_eq!(
        found.choices[1],
        "Fix critical findings, then release this week"
    );
    assert_eq!(found.choices[2], "Fix every finding before any release");
    assert_eq!(
        serde_json::to_value(&found).unwrap()["details"],
        serde_json::json!([
            "",
            "",
            "Fix all findings.\nRun the full retest.",
            "Ship the build to a small share of users first, watch crash and error reports, and widen the rollout in steps.",
            "",
            ""
        ])
    );
    assert_eq!(found.choices[4..], ["Type something.", "Chat about this"]);
    record["message"]["content"][0]["input"]["questions"][0]["options"][2]["description"] =
        "A different description.".into();
    std::fs::write(&log, format!("{record}\n")).unwrap();
    assert_eq!(read().id, found.id);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_keeps_null_descriptions_plain() {
    let root = scratch();
    let log = root.join("question.jsonl");
    let mut record: serde_json::Value = serde_json::from_str(LOG).unwrap();
    record["message"]["content"][0]["input"]["questions"][0]["options"][0]["description"] =
        serde_json::Value::Null;
    std::fs::write(&log, format!("{record}\n")).unwrap();
    let found = agent_prompt(Some("claude"), TALL, || None, Some(&log))
        .expect("a null description must leave the prompt readable");
    assert_eq!(found.choices[0], "Release on the planned date");
    assert_eq!(serde_json::to_value(&found).unwrap()["details"][0], "");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_keeps_whitespace_descriptions_plain() {
    let root = scratch();
    let log = root.join("question.jsonl");
    let mut record: serde_json::Value = serde_json::from_str(LOG).unwrap();
    record["message"]["content"][0]["input"]["questions"][0]["options"][0]["description"] =
        " \t\n".into();
    std::fs::write(&log, format!("{record}\n")).unwrap();
    let found = agent_prompt(Some("claude"), TALL, || None, Some(&log)).unwrap();
    assert_eq!(found.choices[0], "Release on the planned date");
    assert_eq!(serde_json::to_value(&found).unwrap()["details"][0], "");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn two_question_screen_keeps_the_existing_prompt() {
    let before = whole_prompt(TWO, || None).unwrap();
    assert!(
        serde_json::to_value(&before)
            .unwrap()
            .get("details")
            .is_none()
    );
    assert_eq!(
        before.question,
        "←  ☐ Color  ☐ Size  ✔ Submit  →\nWhich color?"
    );
    assert_eq!(
        before.choices,
        [
            "Red\nRed",
            "Blue\nBlue",
            "Type something.",
            "Chat about this"
        ]
    );
    assert_eq!(
        agent_prompt(Some("claude"), TWO, || None, Some(Path::new("missing-log"))),
        Some(before)
    );
}

#[test]
fn fallback_requires_the_live_numbered_tail_and_preserves_cursor_and_hash() {
    let root = scratch();
    let log = root.join("question.jsonl");
    std::fs::write(&log, LOG).unwrap();
    let read = |screen: &str| agent_prompt(Some("claude"), screen, || None, Some(&log));
    let found = read(TALL).unwrap();
    let moved = read(&TALL.replace("  3. Fix every", "❯ 3. Fix every")).unwrap();
    assert_eq!(moved.cursor, 2);
    assert_eq!(moved.id, found.id);
    let complete = format!(
        "────\n{QUESTION}\n{}\nEnter to select\n",
        found
            .choices
            .iter()
            .enumerate()
            .map(|(index, label)| format!("  {}. {}", index + 1, label.replace('\n', "\n     ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(swarm::screen::prompt(&complete).unwrap().id, found.id);
    for screen in [
        TALL.replace("  3. Fix every", "  1. Fix every"),
        TALL.replace("  6. Chat about this", "  7. Chat about this"),
        TALL.replace("  6. Chat about this", "  6. Different action"),
        TALL.replace("Enter to select · ↑/↓ to navigate · Esc to cancel", ""),
        format!("{TALL}\n❯ Write a message\n"),
    ] {
        assert_eq!(read(&screen), None, "{screen}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_uses_the_latest_open_call_and_one_matching_question() {
    let root = scratch();
    let log = root.join("question.jsonl");
    let read = || agent_prompt(Some("claude"), TALL, || None, Some(&log));
    let newer = LOG.replace("release-choice", "newer-choice").replace(
        "Fix critical findings, then release this week",
        "A newer unmatched label",
    );
    std::fs::write(&log, format!("{LOG}{newer}")).unwrap();
    assert_eq!(read(), None);
    let answer = "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"newer-choice\",\"content\":\"done\"}]}}\n";
    std::fs::write(&log, format!("{LOG}{newer}{answer}")).unwrap();
    assert_eq!(read().unwrap().question, QUESTION);

    let mut record: serde_json::Value = serde_json::from_str(LOG).unwrap();
    let questions = record["message"]["content"][0]["input"]["questions"]
        .as_array_mut()
        .unwrap();
    let matching = questions[0].clone();
    questions.insert(0, serde_json::json!({"question":"Other question", "options":[{"label":"One"},{"label":"Two"}]}));
    std::fs::write(&log, format!("{record}\n")).unwrap();
    assert_eq!(read().unwrap().question, QUESTION);
    record["message"]["content"][0]["input"]["questions"]
        .as_array_mut()
        .unwrap()
        .push(matching);
    std::fs::write(&log, format!("{record}\n")).unwrap();
    assert_eq!(read(), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_reads_only_complete_questions_in_the_capped_log_tail() {
    let root = scratch();
    let log = root.join("question.jsonl");
    let read = || agent_prompt(Some("claude"), TALL, || None, Some(&log));
    let padding = "x".repeat(256 * 1024 + 10);
    std::fs::write(&log, format!("{LOG}{padding}\n")).unwrap();
    assert_eq!(read(), None);
    std::fs::write(&log, format!("{padding}\n{LOG}")).unwrap();
    assert_eq!(read().unwrap().question, QUESTION);
    let mut record: serde_json::Value = serde_json::from_str(LOG).unwrap();
    record["message"]["content"][0]["input"]["questions"][0]["header"] = padding.into();
    std::fs::write(&log, format!("{record}\n")).unwrap();
    assert_eq!(read(), None);
    std::fs::write(&log, "{malformed\n").unwrap();
    assert_eq!(read(), None);
    assert_eq!(
        agent_prompt(Some("claude"), TALL, || None, Some(&root)),
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn fallback_rejects_a_fifo_without_waiting_for_a_writer() {
    let root = scratch();
    let fifo = root.join("question.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    let path = fifo.clone();
    let reader = std::thread::spawn(move || {
        sender
            .send(agent_prompt(Some("claude"), TALL, || None, Some(&path)))
            .unwrap();
    });
    let result = receiver.recv_timeout(std::time::Duration::from_secs(1));
    if result.is_err() {
        // Release the blocked reader so a regression fails without hanging the test suite.
        drop(std::fs::OpenOptions::new().write(true).open(&fifo).unwrap());
    }
    reader.join().unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        result.expect("a FIFO must be rejected before opening it"),
        None
    );
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
fn answer_accepts_the_id_that_the_listing_shows_for_a_tall_question() {
    let home = scratch();
    let root = home.join(".swarm");
    std::fs::create_dir_all(root.join("adapters")).unwrap();
    std::fs::write(root.join("swarm-home"), "swarm\n").unwrap();
    std::fs::write(home.join("screen"), TALL).unwrap();
    let log = home.join("question.jsonl");
    std::fs::write(&log, LOG).unwrap();
    std::fs::write(root.join("adapters/fake.conf"), "self = true\nspawn = true\nring = true\nlist = printf pane\nclose = true\ncapture = cat \"$HOME/screen\"\nscreen = cat \"$HOME/screen\"\nkey = printf '%s' \"$SWARM_KEY\" >> \"$HOME/keys\"\n").unwrap();
    let connection = swarm::store::open(&root.join("swarm.db")).unwrap();
    let session =
        swarm::store::create_session(&connection, "lane", &home, None, Some("fake")).unwrap();
    swarm::store::add_agent(&connection, &session, "coder", "code").unwrap();
    swarm::store::set_pane(&connection, &session, "coder", "pane").unwrap();
    swarm::store::set_provider(&connection, &session, "coder", "claude").unwrap();
    swarm::store::set_log(&connection, &session, "coder", &log).unwrap();

    let output = cli(&home, &session, &["agents", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listing: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(listing["agents"][0]["state"], "waiting");
    assert_eq!(
        listing["agents"][0]["prompt"]["choices"][0],
        "Release on the planned date"
    );
    assert_eq!(
        listing["agents"][0]["prompt"]["details"][0],
        "Release on the planned date and list the open review findings in the release notes."
    );
    let id = listing["agents"][0]["prompt"]["id"]
        .as_str()
        .expect("listing has a prompt id");
    let output = cli(&home, &session, &["answer", "coder", id, "1"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(home.join("keys")).unwrap(), "2");
    std::fs::write(
        &log,
        LOG.replace(QUESTION, "A different question with the same choices"),
    )
    .unwrap();
    let output = cli(&home, &session, &["answer", "coder", id, "1"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("the question changed"));
    assert_eq!(std::fs::read_to_string(home.join("keys")).unwrap(), "2");
    std::fs::remove_dir_all(home).unwrap();
}
