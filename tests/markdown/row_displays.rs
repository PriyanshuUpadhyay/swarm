//! Public-API regression for the display map of transcript rows (Step 5(g) slice S3, contract V1 R1 and R2, V2 item 3,
//! V3 item 1).
//!
//! The painter (`web/src/react/transcript-row.tsx`, rows 42 to 113) checks a row in a fixed order and the first match
//! decides. A `fold` row, a tool row, a `divider`, a `working` or `swarm_ring` row, a shell row, a command row, an
//! `ends_turn` row and every other kind paint no Markdown. A compaction or compact summary paints Markdown only when
//! its text holds a line feed. A user or queued row paints Markdown with line breaks, and an assistant row paints it
//! without. A step inside a fold row is painted the same way. Expected values come from that order and from the public
//! `display` and `display_with_breaks` functions, never from candidate output. Every public outcome of a test is
//! collected before its single final assertion.
//!
//! Escapes: `\n` is LF (0x0A).

use serde_json::{Value, json};
use std::collections::BTreeMap;
use swarm::view::transcript::markdown::display::{Display, display, display_with_breaks};
use swarm::view::transcript::markdown::row_displays;
use swarm::view::transcript::{CommandChip, Row, RowKind, ShellRun};
use swarm::view::{TextPreview, ToolActivity, ToolState};

fn row(kind: RowKind, id: &str, text: &str) -> Row {
    Row::new(kind, text.to_owned(), id.to_owned())
}

fn system(kind: RowKind, id: &str, text: &str, system_kind: &str) -> Row {
    Row {
        system_kind: Some(system_kind.to_owned()),
        ..row(kind, id, text)
    }
}

fn plain(text: &str) -> Display {
    display(text).unwrap()
}

fn with_breaks(text: &str) -> Display {
    display_with_breaks(text).unwrap()
}

fn preview() -> TextPreview {
    TextPreview {
        id: "preview".to_owned(),
        text: String::new(),
        is_truncated: false,
        full_lines: 0,
        full_bytes: 0,
        hidden_lines: 0,
    }
}

fn tool_activity() -> ToolActivity {
    ToolActivity {
        name: "Bash".to_owned(),
        input: Value::Null,
        output: None,
        diffs: Vec::new(),
        state: ToolState::Waiting,
        command: None,
        path: None,
        duration_seconds: None,
        exit_code: None,
        header_title: "Bash".to_owned(),
        skill_body: None,
    }
}

#[test]
fn row_displays_follow_the_painter_branches() {
    // Each text holds Markdown and a line feed, so a row that wrongly gets an entry shows a different value.
    let step_text = "step **bold**\nnext";
    let compaction_text = "# Summary\nline";
    let summary_text = "- item\nline";
    let user_text = "one\ntwo";
    let queued_text = "three\nfour";
    let assistant_text = "alpha\nbeta *x*";

    let rows = vec![
        // Fold row: no entry. Its assistant step gets one.
        Row {
            steps: vec![row(RowKind::Assistant, "step-assistant", step_text)],
            ..row(RowKind::Fold, "folded-steps", "*folded*\nsteps")
        },
        // Tool row: no entry.
        Row {
            tool: Some(tool_activity()),
            ..row(RowKind::Assistant, "tool-card", "*tool*\ntext")
        },
        row(RowKind::Divider, "context-divider", "*cleared*\nhere"),
        system(
            RowKind::Assistant,
            "working-status",
            "*work*\nhere",
            "working",
        ),
        system(
            RowKind::Assistant,
            "swarm-ring",
            "*ring*\nhere",
            "swarm_ring",
        ),
        // Compaction without a line feed: no entry. With a line feed or as a compact summary: display.
        system(
            RowKind::Assistant,
            "one-line-compaction",
            "*compacted*",
            "compaction",
        ),
        system(
            RowKind::System,
            "long-compaction",
            compaction_text,
            "compaction",
        ),
        system(
            RowKind::System,
            "compact-summary",
            summary_text,
            "compact_summary",
        ),
        // Shell row and command row: no entry.
        Row {
            shell: Some(ShellRun {
                command: None,
                output: preview(),
                exit_code: None,
            }),
            ..row(RowKind::User, "shell-run", "*shell*\nrun")
        },
        Row {
            command: Some(CommandChip {
                name: "/help".to_owned(),
                arguments: String::new(),
                skill_body: None,
                output: None,
            }),
            ..row(RowKind::User, "command-chip", "*command*\nchip")
        },
        // User and queued rows: line breaks.
        row(RowKind::User, "user-message", user_text),
        row(RowKind::Queued, "queued-message", queued_text),
        // A row that ends a turn: no entry.
        Row {
            ends_turn: true,
            ..row(RowKind::Assistant, "turn-end", "*done*\nturn")
        },
        row(RowKind::Notice, "notice", "*notice*\ntext"),
        row(RowKind::Assistant, "assistant-message", assistant_text),
    ];

    let observed = row_displays(&rows);

    let expected = BTreeMap::from([
        ("step-assistant".to_owned(), plain(step_text)),
        ("long-compaction".to_owned(), plain(compaction_text)),
        ("compact-summary".to_owned(), plain(summary_text)),
        ("user-message".to_owned(), with_breaks(user_text)),
        ("queued-message".to_owned(), with_breaks(queued_text)),
        ("assistant-message".to_owned(), plain(assistant_text)),
    ]);
    assert_eq!(observed, expected);
}

#[test]
fn row_displays_serialize_as_the_contract() {
    // The tree is the one `display` gives. The JSON is written from contract R1: `Display` is `{"blocks": [...]}`,
    // `Block` and `Inline` are adjacently tagged (`type` in snake case, `value`), an absent option is null, and an
    // `Align` is a snake case string.
    let rows = vec![
        row(
            RowKind::Assistant,
            "heading-and-link",
            "# Title\n\nSee [docs](https://e.test/x).",
        ),
        row(
            RowKind::Assistant,
            "aligned-table",
            "| a | b | c | d |\n|:--|:-:|--:|---|\n| 1 | 2 | 3 | 4 |",
        ),
        row(RowKind::User, "user-break", "a\nb"),
    ];

    let observed = serde_json::to_value(row_displays(&rows)).unwrap();

    let expected = json!({
        "aligned-table": {"blocks": [{"type": "table", "value": {
            "source": "| a | b | c | d |\n|:--|:-:|--:|---|\n| 1 | 2 | 3 | 4 |",
            "align": ["left", "center", "right", "none"],
            "head": [
                [{"type": "text", "value": "a"}],
                [{"type": "text", "value": "b"}],
                [{"type": "text", "value": "c"}],
                [{"type": "text", "value": "d"}]
            ],
            "rows": [[
                [{"type": "text", "value": "1"}],
                [{"type": "text", "value": "2"}],
                [{"type": "text", "value": "3"}],
                [{"type": "text", "value": "4"}]
            ]]
        }}]},
        "heading-and-link": {"blocks": [
            {"type": "heading", "value": {
                "depth": 1,
                "children": [{"type": "text", "value": "Title"}]
            }},
            {"type": "paragraph", "value": [
                {"type": "text", "value": "See "},
                {"type": "link", "value": {
                    "url": "https://e.test/x",
                    "title": null,
                    "children": [{"type": "text", "value": "docs"}]
                }},
                {"type": "text", "value": "."}
            ]}
        ]},
        "user-break": {"blocks": [{"type": "paragraph", "value": [
            {"type": "text", "value": "a"},
            {"type": "break"},
            {"type": "text", "value": "b"}
        ]}]}
    });
    assert_eq!(observed, expected);
}
