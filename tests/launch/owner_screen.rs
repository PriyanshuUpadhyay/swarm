// Screen cards: a question Swarm cannot read as choices is answered with keys from the chat
// (ADR 0156).

use super::owner::{answer_session, listed_prompt};
use super::*;

const SCREEN_KEYS: [&str; 4] = ["↑", "↓", "Enter", "Esc"];

#[test]
fn a_question_cut_at_the_top_of_the_pane_gets_a_screen_card_that_presses_keys() {
    let home = scratch("screen-card");
    let session = answer_session(&home, "true");
    let conf = home.join(".swarm/adapters/fake.conf");
    let ring = "ring = printf '%s\\n' \"$SWARM_TEXT\" >> \"$HOME/rang\"";
    let fake = std::fs::read_to_string(&conf)
        .unwrap()
        .replace("ring = true", ring);
    std::fs::write(&conf, fake).unwrap();
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "claude").unwrap();
    // The question's top is above the screen, and no log names it. Claude's hook reported the
    // wait.
    let screen = include_str!("../fixtures/screens/claude-question-top-offscreen.txt");
    std::fs::write(home.join("screen"), screen).unwrap();
    connection
        .execute(
            "UPDATE agent SET state = 'blocked', state_source = 'hook', state_at = unixepoch() \
             WHERE session_id = ?1 AND id = 'seat'",
            [&session],
        )
        .unwrap();
    assert_eq!(listed_seat(&home, &session)["state"], "blocked");

    let prompt = listed_prompt(&home, &session);
    assert_eq!(prompt["screen"], true, "{prompt}");
    assert!(
        prompt["question"]
            .as_str()
            .unwrap()
            .contains("4. Release gradually to a small user group")
    );
    assert_eq!(prompt["choices"], serde_json::json!(SCREEN_KEYS));
    let app = chair(&session);
    let output = swarm(&home, &app, &["notices", "--inbox", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let inbox: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(inbox["needs"][0]["prompt"]["id"], prompt["id"], "{inbox}");

    let id = prompt["id"].as_str().unwrap();
    for (index, key) in [(1, "Down"), (2, "Enter"), (3, "Escape")] {
        let output = swarm(&home, &app, &["answer", "seat", id, &index.to_string()]);
        assert!(output.status.success(), "{}", stderr(&output));
        let keys = std::fs::read_to_string(home.join("keys")).unwrap();
        assert_eq!(keys.lines().last(), Some(key));
    }
    let output = swarm_input(
        &home,
        &app,
        &["answer", "seat", id, "--text"],
        Some("ship it"),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(home.join("rang")).unwrap(),
        "ship it\n"
    );

    // A card read before the screen changed presses nothing.
    std::fs::remove_file(home.join("keys")).unwrap();
    std::fs::write(
        home.join("screen"),
        screen.replace("Fix every", "Skip every"),
    )
    .unwrap();
    assert!(
        !swarm(&home, &app, &["answer", "seat", id, "2"])
            .status
            .success()
    );
    assert!(!home.join("keys").exists());
}

#[test]
fn an_idle_agent_whose_input_box_is_gone_shows_its_picker_as_a_screen_card() {
    let home = scratch("screen-picker");
    let session = answer_session(&home, "true");
    let connection = swarm::store::open(&home.join(".swarm/swarm.db")).unwrap();
    swarm::store::set_provider(&connection, &session, "seat", "claude").unwrap();
    std::fs::write(
        home.join("screen"),
        include_str!("../fixtures/screens/claude-idle.txt"),
    )
    .unwrap();
    assert_eq!(listed_seat(&home, &session)["state"], "done");
    assert_eq!(listed_prompt(&home, &session), serde_json::Value::Null);

    // A picker such as /model draws over the input box.
    let picker = " Select model\n\n   Default (recommended)\n ❯ Opus\n   Haiku\n\n Esc to exit\n";
    std::fs::write(home.join("screen"), picker).unwrap();
    let prompt = listed_prompt(&home, &session);
    assert_eq!(prompt["screen"], true, "{prompt}");
    assert!(prompt["question"].as_str().unwrap().contains("❯ Opus"));
}
