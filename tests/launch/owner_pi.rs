// Owner answers to Pi's question and plan dialogs (ADR 0154).

use super::owner::{answer_session, listed_prompt};

use super::*;

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
    // Pi deletes the box's text on C-u.
    let session = answer_session(
        &home,
        "if [ \"$SWARM_KEY\" = C-u ]; then perl -pi -e 's/^> .*$/>/' \"$HOME/screen\"; fi",
    );
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

    // Text left in the pane's box would join the answer, so the app clears the box first.
    std::fs::write(home.join("screen"), screen.replace("\n>\n", "\n> Up\n")).unwrap();
    let output = answer("More");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("keys")).unwrap(),
        "C-e\nC-u\n"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("rang")).unwrap(),
        "Up and Down switch projects\nMore\n"
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
        // A pane too small for any plan row still shows the choices.
        "→ Request changes\n  Approve\nPi plan approval\n",
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
fn a_pi_question_taller_than_the_pane_is_answered_from_the_card() {
    let home = scratch("pi-tall-question");
    let session = answer_session(&home, "true");
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "pi").unwrap();
    std::fs::write(home.join("status"), "state=blocked:kind=question:at=100\n").unwrap();
    // The question's top and the dialog's upper rule are above the pane's first row.
    let screen = include_str!("../fixtures/screens/pi-question-select.txt");
    let clipped = &screen[screen.find("Pick a color.").unwrap() + "Pick a color.".len()..];
    std::fs::write(home.join("screen"), clipped).unwrap();
    let log = home.join("pi.jsonl");
    swarm::store::set_log(&connection, &session, "seat", &log).unwrap();
    let call = serde_json::json!({"type": "message", "message": {"role": "assistant", "content": [
        {"type": "toolCall", "id": "call-1", "name": "ask_user_question",
         "arguments": {"question": "Many rows of context.\nPick a color.", "choices": ["Amber", "Blue"]}}
    ]}});
    std::fs::write(&log, format!("{call}\n")).unwrap();
    let app = chair(&session);
    assert_eq!(listed_seat(&home, &session)["state_kind"], "question");
    let output = swarm(&home, &app, &["notices", "--inbox", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let inbox: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let prompt = &inbox["needs"][0]["prompt"];
    assert_eq!(
        prompt["question"], "Many rows of context.\nPick a color.",
        "{inbox}"
    );
    assert_eq!(prompt["choices"], serde_json::json!(["Amber", "Blue"]));
    let id = prompt["id"].as_str().unwrap();
    let output = swarm(&home, &app, &["answer", "seat", id, "0"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("keys")).unwrap(),
        "Enter\n"
    );
}
