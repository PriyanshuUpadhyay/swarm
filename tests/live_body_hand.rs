//! S10e-3: GREEN-only; proof by faults. No RED claim on the absent API.
use serde_json::Value;
use swarm::view::transcript::markdown::display::{Block, Display, Inline, LiveBody, live_body};

#[test]
fn s10e3_hand_live_thresholds_use_utf16_and_full_length_cut() {
    let inputs: Value =
        serde_json::from_str(include_str!("fixtures/s10e3/threshold-hand.json")).unwrap();
    let paragraph = |text: &str| Block::Paragraph(vec![Inline::Text(text.into())]);
    let cases = [
        ("no-display", None),
        (
            "no-later",
            Some(Display {
                blocks: vec![paragraph("ab")],
            }),
        ),
        (
            "original-later-between",
            Some(Display {
                blocks: vec![paragraph("ab"), Block::Later("block"), paragraph("c")],
            }),
        ),
        (
            "trailing-later",
            Some(Display {
                blocks: vec![paragraph("ab"), Block::Later("block")],
            }),
        ),
        (
            "later-first",
            Some(Display {
                blocks: vec![Block::Later("block"), paragraph("c")],
            }),
        ),
        (
            "utf16-before-later",
            Some(Display {
                blocks: vec![
                    paragraph("\u{1f600}"),
                    Block::Later("block"),
                    paragraph("c"),
                ],
            }),
        ),
        (
            "nested-inline-later",
            Some(Display {
                blocks: vec![Block::Blockquote(vec![Block::Paragraph(vec![
                    Inline::Strong(vec![
                        Inline::Text("ab".into()),
                        Inline::Later("inline"),
                        Inline::Text("c".into()),
                    ]),
                ])])],
            }),
        ),
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
            input["display"],
            "{id}: exact frozen HAND Display"
        );
        let choice: LiveBody = live_body(display.as_ref());
        assert_eq!(
            serde_json::to_value(choice).unwrap(),
            input["live_body"],
            "{id}"
        );
    }
}
