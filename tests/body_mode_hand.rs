//! S10e-2 B: GREEN-only; proof by faults. No RED claim on the old API.
use serde_json::{Value, json};
use swarm::view::transcript::markdown::display::{Block, BodyMode, Display, Inline, body_mode};

#[test]
fn s10e2_hand_later_choices_and_no_later_control() {
    let inputs: Value =
        serde_json::from_str(include_str!("fixtures/s10e2/inputs-hand.json")).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/s10e2/hand-expected.json")).unwrap();
    let patch = || Block::Code {
        label: Some("diff".into()),
        text: "@@ -1 +1 @@\n-a\n+b\n".into(),
        diff: Some(
            json!({"prefix":[{"kind":"ctx","text":"later metadata is not a Display node"}]}),
        ),
    };
    let paragraph = |text: &str| Block::Paragraph(vec![Inline::Text(text.into())]);
    let mixed = || vec![paragraph("before"), patch(), paragraph("after")];
    let mut with_later = mixed();
    with_later.push(Block::Later("block"));
    let cases = [
        (
            "hand-later-block",
            Display {
                blocks: vec![Block::Later("block")],
            },
        ),
        (
            "hand-later-inline",
            Display {
                blocks: vec![Block::Blockquote(vec![Block::Paragraph(vec![
                    Inline::Text("a".into()),
                    Inline::Later("inline"),
                ])])],
            },
        ),
        ("hand-mixed-patch-later", Display { blocks: with_later }),
        ("hand-mixed-patch", Display { blocks: mixed() }),
    ];
    for (id, display) in cases {
        let input = inputs
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&display).unwrap(),
            input["display"]["p"],
            "{id}: must use exact frozen HAND input"
        );
        let choice = expected
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap();
        let mode = match choice["body"]["kind"].as_str().unwrap() {
            "tree" => BodyMode::Tree,
            "plain" => BodyMode::Plain,
            _ => unreachable!(),
        };
        assert_eq!(body_mode(Some(&display)), mode, "{id}");
    }
}
