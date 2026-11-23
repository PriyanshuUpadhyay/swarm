// Owner answers, keys, and prompt state tests.

use super::*;

/// A session with a chair and one child on a fake adapter whose screen is `$HOME/screen` and
/// whose `key` verb appends to `$HOME/keys`. Its history is the same screen. `key_then` runs after each key, to change the screen.
fn answer_session(home: &Path, key_then: &str) -> String {
    std::fs::create_dir_all(home.join(".swarm/adapters")).unwrap();
    std::fs::write(
        home.join(".swarm/adapters/fake.conf"),
        format!(
            "self = printf chair\nspawn = printf pane\nring = true\nlist = printf 'pane claude\\n'\n\
             close = true\ncapture = cat \"$HOME/screen\"\nscreen = cat \"$HOME/screen\"\n\
             status = cat \"$HOME/status\" 2>/dev/null || true\n\
             key = printf '%s\\n' \"$SWARM_KEY\" >> \"$HOME/keys\"; {key_then}\n"
        ),
    )
    .unwrap();
    let fake = [("SWARM_ADAPTER", "fake")];
    let session = swarm(home, &fake, &["session", "new", "lane"]);
    assert!(session.status.success(), "{}", stderr(&session));
    let session = String::from_utf8(session.stdout)
        .unwrap()
        .trim()
        .to_string();
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    for args in [
        &["agent", "add", "orchestrator", "orchestrator"][..],
        &["spawn", "seat", "coder", "--", "true"],
    ] {
        let output = swarm(home, &chair, args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
    }
    // A text screen is read by its provider's patterns; `spawn --provider` needs an account.
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "claude").unwrap();
    session
}

fn listed_prompt(home: &Path, session: &str) -> serde_json::Value {
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(home, &chair, &["agents", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    list["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["id"] == "seat")
        .unwrap()["prompt"]
        .clone()
}

const ARROWS: &str =
    "● Checking the folder.\n Do you want to proceed?\n > Yes\n   No\n tab amend\n";
const FOLDER_TRUST: &str =
    "  Trust this folder?\n\n› 1. Trust and continue\n  2. Back\n\n  enter continue · esc back\n";

#[test]
fn the_owner_answers_a_listed_question_with_its_choice_key() {
    let home = scratch("answer");
    let session = answer_session(&home, "printf 'done\\n' > \"$HOME/screen\"");
    std::fs::write(home.join("screen"), PERMISSION).unwrap();
    let prompt = listed_prompt(&home, &session);
    assert_eq!(prompt["choices"], serde_json::json!(["Yes", "No"]));
    let id = prompt["id"].as_str().unwrap().to_string();
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let keys = || std::fs::read_to_string(home.join("keys")).unwrap_or_default();

    // Inside an agent pane nobody answers for the owner.
    let mut in_pane = app.to_vec();
    in_pane.push(("TMUX_PANE", "%9"));
    let output = swarm(&home, &in_pane, &["answer", "seat", &id, "1"]);
    assert!(
        stderr(&output).contains("only the owner answers"),
        "{}",
        stderr(&output)
    );
    // A stale id and a missing choice send nothing.
    let output = swarm(&home, &app, &["answer", "seat", "0000000000000000", "1"]);
    assert!(
        stderr(&output).contains("the question changed"),
        "{}",
        stderr(&output)
    );
    let output = swarm(&home, &app, &["answer", "seat", &id, "2"]);
    assert!(
        stderr(&output).contains("no choice 2"),
        "{}",
        stderr(&output)
    );
    assert_eq!(keys(), "");

    // The digit picks the choice; the screen moves on, so no Enter follows.
    let output = swarm(&home, &app, &["answer", "seat", &id, "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(keys(), "2\n");
    assert_eq!(listed_prompt(&home, &session), serde_json::Value::Null);
}

#[test]
fn an_arrow_answer_sends_enter_only_once_the_cursor_moved() {
    let home = scratch("answer-arrows");
    let session = answer_session(&home, "cp \"$HOME/moved\" \"$HOME/screen\"");
    std::fs::write(home.join("screen"), ARROWS).unwrap();
    std::fs::write(
        home.join("moved"),
        ARROWS.replace(" > Yes\n   No", "   Yes\n > No"),
    )
    .unwrap();
    let id = listed_prompt(&home, &session)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(&home, &app, &["answer", "seat", &id, "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("keys")).unwrap(),
        "Down\nEnter\n"
    );

    // A cursor left on another choice means the screen took some other key: no Enter.
    std::fs::remove_file(home.join("keys")).unwrap();
    std::fs::write(home.join("screen"), ARROWS).unwrap();
    std::fs::write(home.join("moved"), ARROWS).unwrap();
    let output = swarm(&home, &app, &["answer", "seat", &id, "1"]);
    assert!(
        stderr(&output).contains("changed while the answer was sent"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        std::fs::read_to_string(home.join("keys")).unwrap(),
        "Down\n"
    );
}

#[test]
fn the_owner_answers_a_pi_question_from_the_notice_inbox() {
    let home = scratch("pi-question");
    let session = answer_session(
        &home,
        "case \"$SWARM_KEY\" in Down) cp \"$HOME/moved\" \"$HOME/screen\";; Enter) \
         cp \"$HOME/done\" \"$HOME/screen\"; printf 'state=done:at=%s\\n' \"$(date +%s)\" > \"$HOME/status\";; esac",
    );
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "pi").unwrap();
    let screen = include_str!("../fixtures/screens/pi-question-select.txt");
    std::fs::write(home.join("screen"), screen).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        home.join("status"),
        format!("state=blocked:kind=question:at={now}\n"),
    )
    .unwrap();
    std::fs::write(
        home.join("moved"),
        screen.replace(" → Amber\n   Blue", "   Amber\n → Blue"),
    )
    .unwrap();
    std::fs::write(
        home.join("done"),
        include_str!("../fixtures/screens/pi-idle.txt"),
    )
    .unwrap();
    let app = chair(&session);
    let listed = listed_seat(&home, &session);
    assert_eq!(listed["state"], "blocked", "{listed}");
    assert_eq!(listed["state_kind"], "question", "{listed}");
    let output = swarm(&home, &app, &["notices", "--inbox", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let inbox: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let need = &inbox["needs"][0];
    assert_eq!(need["agent"], "seat", "{inbox}");
    assert_eq!(need["kind"], "question", "{inbox}");
    let prompt = &need["prompt"];
    assert_eq!(prompt["question"], "Pick a color.", "{prompt}");
    assert_eq!(prompt["choices"], serde_json::json!(["Amber", "Blue"]));
    let id = prompt["id"].as_str().unwrap();
    assert_eq!(listed["prompt"]["id"], id);
    let output = swarm(&home, &app, &["answer", "seat", id, "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(listed_seat(&home, &session)["state"], "done");
}

#[test]
fn the_owner_answers_a_pi_text_question_with_typed_text() {
    let home = scratch("pi-input");
    let session = answer_session(&home, "true");
    let conf = home.join(".swarm/adapters/fake.conf");
    let ring = "ring = printf '%s\\n' \"$SWARM_TEXT\" >> \"$HOME/rang\"";
    let fake = std::fs::read_to_string(&conf)
        .unwrap()
        .replace("ring = true", ring);
    std::fs::write(&conf, fake).unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "pi").unwrap();
    std::fs::write(home.join("status"), "state=blocked:kind=question:at=100\n").unwrap();
    let screen = include_str!("../fixtures/screens/pi-question-input.txt");
    std::fs::write(home.join("screen"), screen).unwrap();
    let prompt = listed_prompt(&home, &session);
    assert_eq!(
        prompt["question"],
        "Please describe the arrow behavior you expect. Which keys\ndo you press?"
    );
    assert_eq!(prompt["input"], true, "{prompt}");
    let id = prompt["id"].as_str().unwrap();
    let app = chair(&session);
    let answer =
        |text: &str| swarm_input(&home, &app, &["answer", "seat", id, "--text"], Some(text));
    let output = answer("Up and Down switch projects\n");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("rang")).unwrap(),
        "Up and Down switch projects\n"
    );

    // Text the owner left in the pane's box would join the answer, so the app does not type.
    std::fs::write(home.join("screen"), screen.replace("\n>\n", "\n> Up\n")).unwrap();
    assert!(!answer("More").status.success());
    assert_eq!(
        std::fs::read_to_string(home.join("rang")).unwrap(),
        "Up and Down switch projects\n"
    );
}

#[test]
fn a_pi_plan_is_answered_from_the_card_with_its_whole_text() {
    let home = scratch("pi-plan");
    let session = answer_session(&home, "true");
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "pi").unwrap();
    std::fs::write(home.join("status"), "state=blocked:kind=question:at=100\n").unwrap();
    let screen = include_str!("../fixtures/screens/pi-question-plan.txt");
    std::fs::write(home.join("screen"), screen).unwrap();
    let app = chair(&session);
    let notice = || {
        assert_eq!(listed_seat(&home, &session)["state_kind"], "question");
        let output = swarm(&home, &app, &["notices", "--inbox", "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["needs"][0]["prompt"]
            .clone()
    };
    let full = notice();
    assert_eq!(full["kind"], "plan");
    assert!(
        full["question"]
            .as_str()
            .unwrap()
            .contains("1. Read the README.\n2. Report the title.")
    );
    assert_eq!(
        full["choices"],
        serde_json::json!(["Request changes", "Approve"])
    );

    // A plan taller than the pane, in a clipped native selector or the pageable overlay, comes
    // whole from the open `approve_plan` call in Pi's log, so the card answers it.
    let log = home.join("pi.jsonl");
    swarm::store::set_log(&connection, &session, "seat", &log).unwrap();
    let plan = (1..=30)
        .map(|n| format!("Step {n}."))
        .collect::<Vec<_>>()
        .join("\n");
    let call = serde_json::json!({"type": "message", "message": {"role": "assistant", "content": [
        {"type": "toolCall", "id": "call-1", "name": "approve_plan", "arguments": {"plan": plan}}
    ]}});
    let result = serde_json::json!({"type": "message", "message": {
        "role": "toolResult", "toolCallId": "call-1", "toolName": "approve_plan"
    }});
    for pane in [
        &screen[screen.find(" 2. Report").unwrap()..],
        "Approve this plan? Rows 1–2 of 30\nStep 1.\nStep 2.\n→ Request changes\n  Approve\nPi plan approval | pageUp/pageDown scroll\n",
    ] {
        std::fs::write(home.join("screen"), pane).unwrap();
        std::fs::write(&log, format!("{call}\n")).unwrap();
        let long = notice();
        assert_eq!(long["kind"], "plan");
        assert!(
            long["question"]
                .as_str()
                .unwrap()
                .contains("Step 1.\nStep 2.")
        );
        assert!(long["question"].as_str().unwrap().contains("Step 30."));
        assert_eq!(
            long["choices"],
            serde_json::json!(["Request changes", "Approve"])
        );
        let _ = std::fs::remove_file(home.join("keys"));
        let output = swarm(
            &home,
            &app,
            &["answer", "seat", long["id"].as_str().unwrap(), "0"],
        );
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(
            std::fs::read_to_string(home.join("keys")).unwrap(),
            "Enter\n"
        );

        // Once the call has a result, the log names no open plan, so the card cannot answer.
        std::fs::write(&log, format!("{call}\n{result}\n")).unwrap();
        let closed = notice();
        assert_eq!(closed["kind"], "plan");
        assert_eq!(closed["choices"], serde_json::json!([]));
        std::fs::remove_file(home.join("keys")).unwrap();
        for id in [long["id"].as_str().unwrap(), closed["id"].as_str().unwrap()] {
            assert!(
                !swarm(&home, &app, &["answer", "seat", id, "0"])
                    .status
                    .success()
            );
        }
        assert!(!home.join("keys").exists());
    }
}

#[test]
fn the_owner_presses_only_up_and_ctrl_u_in_an_agent_pane() {
    let home = scratch("key");
    let session = answer_session(&home, "true");
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let keys = || std::fs::read_to_string(home.join("keys")).unwrap_or_default();

    let mut in_pane = app.to_vec();
    in_pane.push(("HERDR_PANE_ID", "w1:p2"));
    let output = swarm(&home, &in_pane, &["key", "seat", "Up"]);
    assert!(
        stderr(&output).contains("only the owner presses keys"),
        "{}",
        stderr(&output)
    );
    // Escape and C-c stop the agent's turn.
    for key in ["Escape", "C-c"] {
        let output = swarm(&home, &app, &["key", "seat", key]);
        assert!(
            stderr(&output).contains("only Up and C-u"),
            "{key}: {}",
            stderr(&output)
        );
    }
    assert_eq!(keys(), "");

    for key in ["Up", "C-u"] {
        let output = swarm(&home, &app, &["key", "seat", key]);
        assert!(output.status.success(), "{key}: {}", stderr(&output));
    }
    assert_eq!(keys(), "Up\nC-u\n");

    let output = swarm(&home, &app, &["key"]);
    assert!(
        stderr(&output).contains("| key <agent_id> <Up|C-u> |"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_folder_trust_screen_that_nothing_closes_above_is_answered_only_in_the_pane() {
    let home = scratch("answer-trust");
    let session = answer_session(&home, "true");
    std::fs::write(home.join("screen"), FOLDER_TRUST).unwrap();
    assert_eq!(listed_prompt(&home, &session), serde_json::Value::Null);
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let output = swarm(&home, &app, &["answer", "seat", "0000000000000000", "0"]);
    assert!(
        stderr(&output).contains("shows no question now"),
        "{}",
        stderr(&output)
    );
    assert!(!home.join("keys").exists());
}

#[test]
fn a_second_answer_to_the_same_pane_is_refused_and_sends_nothing() {
    let home = scratch("answer-lock");
    let session = answer_session(&home, "true");
    std::fs::write(home.join("screen"), PERMISSION).unwrap();
    let id = listed_prompt(&home, &session)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let app = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    // Another answer to this pane holds its lock.
    let held = std::fs::File::create(home.join(".swarm/answer-pane.lock")).unwrap();
    held.lock().unwrap();
    let output = swarm(&home, &app, &["answer", "seat", &id, "0"]);
    assert!(
        stderr(&output).contains("still being sent"),
        "{}",
        stderr(&output)
    );
    assert!(!home.join("keys").exists());
    drop(held);
    let output = swarm(&home, &app, &["answer", "seat", &id, "0"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(std::fs::read_to_string(home.join("keys")).unwrap(), "1\n");
}

#[test]
fn a_seat_with_no_provider_shows_the_state_that_herdr_reports() {
    let home = scratch("no-provider");
    let session = answer_session(&home, "true");
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    connection
        .execute("UPDATE agent SET provider = NULL WHERE id = 'seat'", [])
        .unwrap();
    let chair = [
        ("SWARM_ADAPTER", "fake"),
        ("SWARM_SESSION_ID", session.as_str()),
        ("SWARM_AGENT_ID", "orchestrator"),
    ];
    let seat = |screen: &str| {
        std::fs::write(home.join("screen"), screen).unwrap();
        let output = swarm(&home, &chair, &["agents", "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let list: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        list["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|agent| agent["id"] == "seat")
            .unwrap()
            .clone()
    };
    // A text screen (tmux) needs a known provider to be read, so the state stays unset.
    let text = seat(PERMISSION);
    assert_eq!(text["state"], serde_json::Value::Null);
    assert_eq!(text["prompt"], serde_json::Value::Null);
    // Herdr's status names no provider.
    let working = seat(r#"{"result":{"agent":{"agent_status":"working"}}}"#);
    assert_eq!(working["state"], "working");
    assert_eq!(working["state_source"], "screen");
    let idle = seat(r#"{"result":{"agent":{"agent_status":"idle"}}}"#);
    assert_eq!(idle["state"], "done");
    let blocked = seat(r#"{"result":{"agent":{"agent_status":"blocked"}}}"#);
    assert_eq!(blocked["state"], "blocked");
}
